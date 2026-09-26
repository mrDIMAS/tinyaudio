//! Linux output device via `alsa`.
//!
//! `libasound.so.2` is loaded at runtime with `dlopen` instead of being linked at build time.
//! This means that a binary using this crate can still start on a machine without ALSA installed
//! (e.g. a minimal container or a headless server); only [`run_output_device`](crate::run_output_device)
//! will fail with an error. It also means that the ALSA development headers are not required to
//! build the crate.

#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "alsa"))]
// Type aliases below mirror the C names from `alsa/pcm.h`.
#![allow(non_camel_case_types)]

use crate::{AudioOutputDevice, BaseAudioOutputDevice, OutputDeviceParameters};
use libloading::Library;
use std::{
    error::Error,
    ffi::{CStr, CString},
    os::raw::{c_char, c_int, c_long, c_uint, c_ulong, c_void},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
};

const ALSA_LIBRARY_NAME: &str = "libasound.so.2";

// Opaque ALSA types.
type snd_pcm_t = c_void;
type snd_pcm_hw_params_t = c_void;
type snd_pcm_sw_params_t = c_void;

// Scalar ALSA types.
type snd_pcm_stream_t = c_int;
type snd_pcm_access_t = c_int;
type snd_pcm_format_t = c_int;
type snd_pcm_uframes_t = c_ulong;
type snd_pcm_sframes_t = c_long;

const SND_PCM_STREAM_PLAYBACK: snd_pcm_stream_t = 0;
const SND_PCM_ACCESS_RW_INTERLEAVED: snd_pcm_access_t = 3;
const SND_PCM_FORMAT_S16_LE: snd_pcm_format_t = 2;

/// Declares a struct holding the ALSA library and the function pointers loaded from it, together
/// with a `load` function that fills it in.
macro_rules! alsa_functions {
    ($( fn $name:ident($($arg:ty),* $(,)?) -> $ret:ty; )*) => {
        struct Alsa {
            $( $name: unsafe extern "C" fn($($arg),*) -> $ret, )*
            // Must be kept alive for as long as the function pointers above are in use. Declared
            // last so that it is dropped last, although the function pointers have no destructor.
            _library: Library,
        }

        impl Alsa {
            fn load() -> Result<Self, Box<dyn Error>> {
                // SAFETY: loading libasound runs its initializers, which have no preconditions.
                let library = unsafe { Library::new(ALSA_LIBRARY_NAME) }.map_err(|err| {
                    format!("Failed to load {ALSA_LIBRARY_NAME}: {err}")
                })?;

                // SAFETY: the signatures declared via the macro match the ALSA C API.
                unsafe {
                    Ok(Self {
                        $(
                            $name: *library
                                .get::<unsafe extern "C" fn($($arg),*) -> $ret>(
                                    concat!(stringify!($name), "\0").as_bytes(),
                                )
                                .map_err(|err| {
                                    format!(
                                        "Failed to load `{}` from {ALSA_LIBRARY_NAME}: {err}",
                                        stringify!($name)
                                    )
                                })?,
                        )*
                        _library: library,
                    })
                }
            }
        }
    };
}

alsa_functions! {
    fn snd_strerror(c_int) -> *const c_char;
    fn snd_pcm_open(*mut *mut snd_pcm_t, *const c_char, snd_pcm_stream_t, c_int) -> c_int;
    fn snd_pcm_close(*mut snd_pcm_t) -> c_int;
    fn snd_pcm_prepare(*mut snd_pcm_t) -> c_int;
    fn snd_pcm_writei(*mut snd_pcm_t, *const c_void, snd_pcm_uframes_t) -> snd_pcm_sframes_t;
    fn snd_pcm_recover(*mut snd_pcm_t, c_int, c_int) -> c_int;
    fn snd_pcm_hw_params_malloc(*mut *mut snd_pcm_hw_params_t) -> c_int;
    fn snd_pcm_hw_params_free(*mut snd_pcm_hw_params_t) -> ();
    fn snd_pcm_hw_params_any(*mut snd_pcm_t, *mut snd_pcm_hw_params_t) -> c_int;
    fn snd_pcm_hw_params_set_access(*mut snd_pcm_t, *mut snd_pcm_hw_params_t, snd_pcm_access_t) -> c_int;
    fn snd_pcm_hw_params_set_format(*mut snd_pcm_t, *mut snd_pcm_hw_params_t, snd_pcm_format_t) -> c_int;
    fn snd_pcm_hw_params_set_rate_near(*mut snd_pcm_t, *mut snd_pcm_hw_params_t, *mut c_uint, *mut c_int) -> c_int;
    fn snd_pcm_hw_params_set_channels(*mut snd_pcm_t, *mut snd_pcm_hw_params_t, c_uint) -> c_int;
    fn snd_pcm_hw_params_set_period_size_near(*mut snd_pcm_t, *mut snd_pcm_hw_params_t, *mut snd_pcm_uframes_t, *mut c_int) -> c_int;
    fn snd_pcm_hw_params_set_buffer_size_near(*mut snd_pcm_t, *mut snd_pcm_hw_params_t, *mut snd_pcm_uframes_t) -> c_int;
    fn snd_pcm_hw_params(*mut snd_pcm_t, *mut snd_pcm_hw_params_t) -> c_int;
    fn snd_pcm_sw_params_malloc(*mut *mut snd_pcm_sw_params_t) -> c_int;
    fn snd_pcm_sw_params_current(*mut snd_pcm_t, *mut snd_pcm_sw_params_t) -> c_int;
    fn snd_pcm_sw_params_set_avail_min(*mut snd_pcm_t, *mut snd_pcm_sw_params_t, snd_pcm_uframes_t) -> c_int;
    fn snd_pcm_sw_params_set_start_threshold(*mut snd_pcm_t, *mut snd_pcm_sw_params_t, snd_pcm_uframes_t) -> c_int;
    fn snd_pcm_sw_params(*mut snd_pcm_t, *mut snd_pcm_sw_params_t) -> c_int;
}

impl Alsa {
    fn err_code_to_string(&self, err_code: c_int) -> String {
        // SAFETY: `snd_strerror` returns a pointer to a static string for any error code.
        unsafe {
            CStr::from_ptr((self.snd_strerror)(err_code) as *const _)
                .to_string_lossy()
                .into_owned()
        }
    }

    fn check(&self, err_code: c_int) -> Result<(), Box<dyn Error>> {
        if err_code < 0 {
            Err(self.err_code_to_string(err_code).into())
        } else {
            Ok(())
        }
    }
}

pub struct AlsaSoundDevice {
    alsa: Arc<Alsa>,
    playback_device: *mut snd_pcm_t,
    thread_handle: Option<JoinHandle<()>>,
    is_running: Arc<AtomicBool>,
}

unsafe impl Send for AlsaSoundDevice {}

impl BaseAudioOutputDevice for AlsaSoundDevice {}

impl AudioOutputDevice for AlsaSoundDevice {
    fn new<C>(params: OutputDeviceParameters, data_callback: C) -> Result<Self, Box<dyn Error>>
    where
        C: FnMut(&mut [f32]) + Send + 'static,
        Self: Sized,
    {
        let alsa = Arc::new(Alsa::load()?);

        unsafe {
            let name = CString::new("default").unwrap();
            let frame_count = params.channel_sample_count;
            let mut playback_device = std::ptr::null_mut();
            alsa.check((alsa.snd_pcm_open)(
                &mut playback_device,
                name.as_ptr() as *const _,
                SND_PCM_STREAM_PLAYBACK,
                0,
            ))?;
            let mut hw_params = std::ptr::null_mut();
            alsa.check((alsa.snd_pcm_hw_params_malloc)(&mut hw_params))?;
            alsa.check((alsa.snd_pcm_hw_params_any)(playback_device, hw_params))?;
            let access = SND_PCM_ACCESS_RW_INTERLEAVED;
            alsa.check((alsa.snd_pcm_hw_params_set_access)(
                playback_device,
                hw_params,
                access,
            ))?;
            alsa.check((alsa.snd_pcm_hw_params_set_format)(
                playback_device,
                hw_params,
                SND_PCM_FORMAT_S16_LE,
            ))?;
            let mut exact_rate = params.sample_rate as c_uint;
            alsa.check((alsa.snd_pcm_hw_params_set_rate_near)(
                playback_device,
                hw_params,
                &mut exact_rate,
                std::ptr::null_mut(),
            ))?;
            alsa.check((alsa.snd_pcm_hw_params_set_channels)(
                playback_device,
                hw_params,
                params.channels_count as c_uint,
            ))?;
            let mut _exact_period = frame_count as snd_pcm_uframes_t;
            let mut _direction = 0;
            alsa.check((alsa.snd_pcm_hw_params_set_period_size_near)(
                playback_device,
                hw_params,
                &mut _exact_period,
                &mut _direction,
            ))?;
            let mut exact_size = (frame_count * 2) as snd_pcm_uframes_t;
            alsa.check((alsa.snd_pcm_hw_params_set_buffer_size_near)(
                playback_device,
                hw_params,
                &mut exact_size,
            ))?;
            alsa.check((alsa.snd_pcm_hw_params)(playback_device, hw_params))?;
            (alsa.snd_pcm_hw_params_free)(hw_params);
            let mut sw_params = std::ptr::null_mut();
            alsa.check((alsa.snd_pcm_sw_params_malloc)(&mut sw_params))?;
            alsa.check((alsa.snd_pcm_sw_params_current)(playback_device, sw_params))?;
            alsa.check((alsa.snd_pcm_sw_params_set_avail_min)(
                playback_device,
                sw_params,
                frame_count as snd_pcm_uframes_t,
            ))?;
            alsa.check((alsa.snd_pcm_sw_params_set_start_threshold)(
                playback_device,
                sw_params,
                frame_count as snd_pcm_uframes_t,
            ))?;
            alsa.check((alsa.snd_pcm_sw_params)(playback_device, sw_params))?;
            alsa.check((alsa.snd_pcm_prepare)(playback_device))?;

            let is_running = Arc::new(AtomicBool::new(true));

            let thread_handle = DataSender {
                alsa: alsa.clone(),
                playback_device,
                callback: data_callback,
                data_buffer: vec![0.0f32; params.channel_sample_count * params.channels_count],
                output_buffer: vec![0i16; params.channel_sample_count * params.channels_count],
                is_running: is_running.clone(),
                params,
            }
            .run_in_thread()?;

            Ok(Self {
                alsa,
                playback_device,
                is_running,
                thread_handle: Some(thread_handle),
            })
        }
    }
}

impl Drop for AlsaSoundDevice {
    fn drop(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);

        self.thread_handle
            .take()
            .expect("Alsa thread must exist!")
            .join()
            .unwrap();

        unsafe {
            (self.alsa.snd_pcm_close)(self.playback_device);
        }
    }
}

struct DataSender<C> {
    alsa: Arc<Alsa>,
    playback_device: *mut snd_pcm_t,
    callback: C,
    data_buffer: Vec<f32>,
    output_buffer: Vec<i16>,
    is_running: Arc<AtomicBool>,
    params: OutputDeviceParameters,
}

unsafe impl<C> Send for DataSender<C> {}

impl<C> DataSender<C>
where
    C: FnMut(&mut [f32]) + Send + 'static,
{
    pub fn run_in_thread(mut self) -> Result<JoinHandle<()>, Box<dyn Error>> {
        Ok(std::thread::Builder::new()
            .name("AlsaDataSender".to_string())
            .spawn(move || self.run_send_loop())?)
    }

    pub fn run_send_loop(&mut self) {
        while self.is_running.load(Ordering::SeqCst) {
            (self.callback)(&mut self.data_buffer);

            debug_assert_eq!(self.data_buffer.len(), self.output_buffer.len());
            for (in_sample, out_sample) in
                self.data_buffer.iter().zip(self.output_buffer.iter_mut())
            {
                *out_sample = (*in_sample * i16::MAX as f32) as i16;
            }

            'try_loop: for _ in 0..10 {
                unsafe {
                    let err = (self.alsa.snd_pcm_writei)(
                        self.playback_device,
                        self.output_buffer.as_ptr() as *const _,
                        self.params.channel_sample_count as snd_pcm_uframes_t,
                    ) as i32;

                    if err < 0 {
                        // Try to recover from any errors and re-send data.
                        (self.alsa.snd_pcm_recover)(self.playback_device, err, 1);
                    } else {
                        break 'try_loop;
                    }
                }
            }
        }
    }
}
