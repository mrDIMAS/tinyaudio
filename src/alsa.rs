//! Linux output device via `alsa`.

#![cfg(target_os = "linux")]

use crate::{AudioOutputDevice, BaseAudioOutputDevice, OutputDeviceParameters};
use alsa_sys::*;
use std::{
    error::Error,
    ffi::{CStr, CString},
    os::raw::c_int,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
};

pub struct AlsaSoundDevice {
    playback_device: *mut snd_pcm_t,
    thread_handle: Option<JoinHandle<()>>,
    is_running: Arc<AtomicBool>,
}

unsafe impl Send for AlsaSoundDevice {}

pub fn err_code_to_string(err_code: c_int) -> String {
    unsafe {
        let message = CStr::from_ptr(snd_strerror(err_code) as *const _)
            .to_bytes()
            .to_vec();
        String::from_utf8(message).unwrap()
    }
}

pub fn check(err_code: c_int) -> Result<(), Box<dyn Error>> {
    if err_code < 0 {
        Err(err_code_to_string(err_code).into())
    } else {
        Ok(())
    }
}

impl BaseAudioOutputDevice for AlsaSoundDevice {}

impl AudioOutputDevice for AlsaSoundDevice {
    fn new<C>(params: OutputDeviceParameters, data_callback: C) -> Result<Self, Box<dyn Error>>
    where
        C: FnMut(&mut [f32]) + Send + 'static,
        Self: Sized,
    {
        unsafe {
            let name = CString::new("default").unwrap();
            let frame_count = params.channel_sample_count;
            let mut playback_device = std::ptr::null_mut();
            check(snd_pcm_open(
                &mut playback_device,
                name.as_ptr() as *const _,
                SND_PCM_STREAM_PLAYBACK,
                0,
            ))?;
            let mut hw_params = std::ptr::null_mut();
            check(snd_pcm_hw_params_malloc(&mut hw_params))?;
            check(snd_pcm_hw_params_any(playback_device, hw_params))?;
            let access = SND_PCM_ACCESS_RW_INTERLEAVED;
            check(snd_pcm_hw_params_set_access(
                playback_device,
                hw_params,
                access,
            ))?;
            check(snd_pcm_hw_params_set_format(
                playback_device,
                hw_params,
                SND_PCM_FORMAT_S16_LE,
            ))?;
            let mut exact_rate = params.sample_rate as ::std::os::raw::c_uint;
            check(snd_pcm_hw_params_set_rate_near(
                playback_device,
                hw_params,
                &mut exact_rate,
                std::ptr::null_mut(),
            ))?;
            check(snd_pcm_hw_params_set_channels(
                playback_device,
                hw_params,
                params.channels_count as ::std::os::raw::c_uint,
            ))?;
            let mut _exact_period = frame_count as snd_pcm_uframes_t;
            let mut _direction = 0;
            check(snd_pcm_hw_params_set_period_size_near(
                playback_device,
                hw_params,
                &mut _exact_period,
                &mut _direction,
            ))?;
            let mut exact_size = (frame_count * 2) as ::std::os::raw::c_ulong;
            check(snd_pcm_hw_params_set_buffer_size_near(
                playback_device,
                hw_params,
                &mut exact_size,
            ))?;
            check(snd_pcm_hw_params(playback_device, hw_params))?;
            snd_pcm_hw_params_free(hw_params);
            let mut sw_params = std::ptr::null_mut();
            check(snd_pcm_sw_params_malloc(&mut sw_params))?;
            check(snd_pcm_sw_params_current(playback_device, sw_params))?;
            check(snd_pcm_sw_params_set_avail_min(
                playback_device,
                sw_params,
                frame_count as ::std::os::raw::c_ulong,
            ))?;
            check(snd_pcm_sw_params_set_start_threshold(
                playback_device,
                sw_params,
                frame_count as ::std::os::raw::c_ulong,
            ))?;
            check(snd_pcm_sw_params(playback_device, sw_params))?;
            check(snd_pcm_prepare(playback_device))?;

            let is_running = Arc::new(AtomicBool::new(true));

            let thread_handle = DataSender {
                playback_device,
                callback: data_callback,
                data_buffer: vec![0.0f32; params.channel_sample_count * params.channels_count],
                output_buffer: vec![0i16; params.channel_sample_count * params.channels_count],
                is_running: is_running.clone(),
                params,
            }
            .run_in_thread()?;

            Ok(Self {
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
            snd_pcm_close(self.playback_device);
        }
    }
}

struct DataSender<C> {
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
                    let err = snd_pcm_writei(
                        self.playback_device,
                        self.output_buffer.as_ptr() as *const _,
                        self.params.channel_sample_count as ::std::os::raw::c_ulong,
                    ) as i32;

                    if err < 0 {
                        // Try to recover from any errors and re-send data.
                        snd_pcm_recover(self.playback_device, err, 1);
                    } else {
                        break 'try_loop;
                    }
                }
            }
        }
    }
}
