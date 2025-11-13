//! Linux output device via `PulseAudio`.

#![cfg(all(target_os = "linux", feature = "pulse"))]
#![cfg_attr(feature = "alsa", allow(dead_code))]

use crate::{AudioOutputDevice, BaseAudioOutputDevice, OutputDeviceParameters};
use libpulse_sys::*;
use std::{
    any::Any,
    cell::Cell,
    error::Error,
    ffi::{c_void, CStr},
    panic::{self, AssertUnwindSafe},
    ptr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
};

pub struct PulseSoundDevice {
    thread_handle: Option<JoinHandle<Result<(), String>>>,
    is_running: Arc<AtomicBool>,
}

impl Drop for PulseSoundDevice {
    fn drop(&mut self) {
        self.is_running.store(false, Ordering::Relaxed);
        let res = self
            .thread_handle
            .take()
            .expect("PulseAudio thread must exist!")
            .join()
            // propagate panic
            .unwrap();

        if let Err(_error) = res {
            // The error from the PulseAudio thread,
            // can be printed or returned if needed
        }
    }
}

impl BaseAudioOutputDevice for PulseSoundDevice {}

impl AudioOutputDevice for PulseSoundDevice {
    fn new<C>(params: OutputDeviceParameters, data_callback: C) -> Result<Self, Box<dyn Error>>
    where
        C: FnMut(&mut [f32]) + Send + 'static,
        Self: Sized,
    {
        let is_running = Arc::new(AtomicBool::new(true));
        let thread_handle = std::thread::Builder::new()
            .name("PulseAudioThread".to_string())
            .spawn({
                let is_running = is_running.clone();
                move || run(params, is_running, data_callback)
            })?;

        Ok(Self {
            thread_handle: Some(thread_handle),
            is_running,
        })
    }
}

fn run<C>(
    params: OutputDeviceParameters,
    is_running: Arc<AtomicBool>,
    mut data_callback: C,
) -> Result<(), String>
where
    C: FnMut(&mut [f32]) + 'static,
{
    unsafe {
        let mainloop = pa_mainloop_new();
        if mainloop.is_null() {
            return Err("failed to create PulseAudio mainloop".to_owned());
        }

        let _free_mainloop = defer(|| pa_mainloop_free(mainloop));

        let api = pa_mainloop_get_api(mainloop);
        if api.is_null() {
            return Err("failed to get PulseAudio mainloop api".to_owned());
        }

        let context = pa_context_new(api, "default\0".as_ptr().cast());
        if context.is_null() {
            return Err("failed to create PulseAudio context".to_owned());
        }

        let _unref_context = defer(|| {
            pa_context_disconnect(context);
            pa_context_unref(context);
        });

        check(
            pa_context_connect(context, ptr::null(), PA_CONTEXT_NOFLAGS, ptr::null()),
            context,
        )?;

        loop {
            match pa_context_get_state(context) {
                PA_CONTEXT_FAILED => {
                    return Err("the connection failed or was disconnected".to_owned());
                }
                PA_CONTEXT_TERMINATED => return Ok(()),
                PA_CONTEXT_READY => break,
                _ => {}
            }

            check(pa_mainloop_iterate(mainloop, 1, ptr::null_mut()), context)?;
        }

        let sample_rate = u32::try_from(params.sample_rate)
            .ok()
            .filter(|&sample_rate| sample_rate <= PA_RATE_MAX)
            .ok_or_else(|| "sample rate exceeds maximum value".to_owned())?;

        let channels_count = u8::try_from(params.channels_count)
            .ok()
            .filter(|&channels_count| channels_count <= PA_CHANNELS_MAX)
            .ok_or_else(|| "channels count exceeds maximum value".to_owned())?;

        let spec = pa_sample_spec {
            format: PA_SAMPLE_FLOAT32LE,
            rate: sample_rate,
            channels: channels_count,
        };

        if pa_sample_spec_valid(&spec) == 0 {
            return Err("spec is not valid".to_owned());
        }

        let stream = check_ptr(
            pa_stream_new(
                context,
                "PulseAudio Stream\0".as_ptr().cast(),
                &spec,
                ptr::null(),
            ),
            context,
        )?;

        let _unref_stream = defer(|| {
            pa_stream_disconnect(stream);
            pa_stream_unref(stream);
        });

        if params.channel_sample_count % usize::from(spec.channels) != 0 {
            return Err("the length of the data to write (in bytes) must be in multiples of the stream sample spec frame size".to_owned());
        }

        let mut buffer = vec![0.; params.channel_sample_count];
        let mut callback = move |nbytes, stream| {
            let bytelen = buffer.len() * size_of::<f32>();
            for _ in 0..nbytes / bytelen {
                data_callback(&mut buffer);
                check(
                    pa_stream_write(
                        stream,
                        buffer.as_ptr().cast(),
                        bytelen,
                        None,
                        0,
                        PA_SEEK_RELATIVE,
                    ),
                    context,
                )?;
            }

            Ok(())
        };

        enum WriteState {
            Ok,
            PulseError(String),
            Panicked(Box<dyn Any + Send + 'static>),
        }

        struct WriteCallback<'cb> {
            callback: &'cb mut dyn FnMut(usize, *mut pa_stream) -> Result<(), String>,
            state: &'cb Cell<WriteState>,
        }

        extern "C" fn write_cb(stream: *mut pa_stream, nbytes: usize, userdata: *mut c_void) {
            unsafe {
                let cb_mut: &mut WriteCallback<'_> = &mut *userdata.cast();

                let res =
                    panic::catch_unwind(AssertUnwindSafe(|| (cb_mut.callback)(nbytes, stream)));

                let state = match res {
                    Ok(Ok(())) => WriteState::Ok,
                    Ok(Err(error)) => WriteState::PulseError(error),
                    Err(message) => WriteState::Panicked(message),
                };

                cb_mut.state.set(state);
            }
        }

        let state = Cell::new(WriteState::Ok);
        let mut write = WriteCallback {
            callback: &mut callback,
            state: &state,
        };

        pa_stream_set_write_callback(
            stream,
            Some(write_cb),
            (&mut write as *mut WriteCallback<'_>).cast(),
        );

        // Unset the pointer to `WriteCallback`
        // so that it isn't called after the function returns.
        // This also allows safely drop the `write` value from the stack after.
        let _unset_write_callback =
            defer(|| pa_stream_set_write_callback(stream, None, ptr::null_mut()));

        check(
            pa_stream_connect_playback(
                stream,
                ptr::null(),
                ptr::null(),
                PA_STREAM_START_CORKED,
                ptr::null(),
                ptr::null_mut(),
            ),
            context,
        )?;

        while is_running.load(Ordering::Relaxed) {
            check(pa_mainloop_iterate(mainloop, 1, ptr::null_mut()), context)?;

            if pa_stream_is_corked(stream) == 1 {
                pa_stream_cork(stream, 0, None, ptr::null_mut());
            }

            match state.replace(WriteState::Ok) {
                WriteState::Ok => {}
                WriteState::PulseError(error) => return Err(error),
                WriteState::Panicked(message) => panic::panic_any(message),
            }
        }

        Ok(())
    }
}

fn check(code: i32, context: *const pa_context) -> Result<(), String> {
    if code < 0 {
        Err(context_error(context))
    } else {
        Ok(())
    }
}

fn check_ptr<T>(ptr: *mut T, context: *const pa_context) -> Result<*mut T, String> {
    if ptr.is_null() {
        Err(context_error(context))
    } else {
        Ok(ptr)
    }
}

fn context_error(context: *const pa_context) -> String {
    unsafe {
        let error = pa_context_errno(context);
        CStr::from_ptr(pa_strerror(error))
            .to_string_lossy()
            .into_owned()
    }
}

fn defer<F>(f: F) -> impl Drop
where
    F: Fn(),
{
    struct Defer<F>(F)
    where
        F: Fn();

    impl<F> Drop for Defer<F>
    where
        F: Fn(),
    {
        fn drop(&mut self) {
            (self.0)();
        }
    }

    Defer(f)
}
