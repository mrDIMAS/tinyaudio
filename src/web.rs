//! WebAssembly output device via `WebAudio`

#![cfg(all(target_os = "unknown", target_arch = "wasm32"))]

use crate::{AudioOutputDevice, BaseAudioOutputDevice, OutputDeviceParameters};
use std::{
    error::Error,
    sync::{Arc, Mutex, RwLock},
};
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{AudioBuffer, AudioContext, AudioContextOptions};

type OnEndedClosure = Arc<RwLock<Option<Closure<dyn FnMut()>>>>;

fn convert_err(err_object: JsValue) -> Box<dyn Error> {
    format!("WebAudio error occurred: {:?}", err_object).into()
}

fn create_audio_context(
    params: &OutputDeviceParameters,
) -> Result<Arc<AudioContext>, Box<dyn Error>> {
    let mut options = AudioContextOptions::new();

    options.sample_rate(params.sample_rate as f32);

    let audio_context = AudioContext::new_with_context_options(&options).map_err(convert_err)?;

    Ok(Arc::new(audio_context))
}

fn create_buffer(
    audio_context: &AudioContext,
    params: &OutputDeviceParameters,
) -> Result<AudioBuffer, Box<dyn Error>> {
    Ok(audio_context
        .create_buffer(
            params.channels_count as u32,
            params.channel_sample_count as u32,
            params.sample_rate as f32,
        )
        .map_err(convert_err)?)
}

fn write_samples(
    buffer: &AudioBuffer,
    channels_count: usize,
    interleaved_data_buffer: &[f32],
    temp_samples: &mut Vec<f32>,
) {
    for channel_index in 0..channels_count {
        temp_samples.clear();
        for samples in interleaved_data_buffer.chunks(channels_count) {
            temp_samples.push(samples[channel_index]);
        }
        buffer
            .copy_to_channel(&temp_samples, channel_index as i32)
            .unwrap();
    }
}

fn create_buffer_source(
    audio_context: &AudioContext,
    buffer: &AudioBuffer,
    start_time: f64,
    onended_closure: &OnEndedClosure,
) {
    let source = audio_context.create_buffer_source().unwrap();
    source.set_buffer(Some(&buffer));
    source
        .connect_with_audio_node(&audio_context.destination())
        .unwrap();
    source.set_onended(Some(
        onended_closure
            .read()
            .unwrap()
            .as_ref()
            .unwrap()
            .as_ref()
            .unchecked_ref(),
    ));
    source.start_with_when(start_time).unwrap();
}

pub struct WebAudioDevice {
    audio_context: Arc<AudioContext>,
}

impl BaseAudioOutputDevice for WebAudioDevice {}

unsafe impl Send for WebAudioDevice {}

impl AudioOutputDevice for WebAudioDevice {
    fn new<C>(params: OutputDeviceParameters, data_callback: C) -> Result<Self, Box<dyn Error>>
    where
        C: FnMut(&mut [f32]) + Send + 'static,
        Self: Sized,
    {
        let window = web_sys::window().ok_or_else(|| "Failed to fetch main window.")?;
        let audio_context = create_audio_context(&params)?;
        let callback = Arc::new(Mutex::new(data_callback));

        let time = Arc::new(RwLock::new(0.0f64));

        let buffer_duration_secs = params.channel_sample_count as f64 / params.sample_rate as f64;
        let time_step_ms = (buffer_duration_secs * 1_000.0) as i32;
        let mut offset_ms = 0;

        for _ in 0..2 {
            let buffer = create_buffer(&audio_context, &params)?;

            let onended_closure: OnEndedClosure = Arc::new(RwLock::new(None));

            let audio_context_clone = audio_context.clone();
            let onended_closure_clone = onended_closure.clone();
            let time = time.clone();
            let callback = callback.clone();

            let mut interleaved_data_buffer =
                vec![0.0f32; params.channel_sample_count * params.channels_count];
            let mut temp_samples = vec![0.0f32; params.channel_sample_count];

            onended_closure
                .write()
                .unwrap()
                .replace(Closure::wrap(Box::new(move || {
                    let current_time = audio_context_clone.current_time();
                    let raw_time = *time.read().unwrap();
                    let start_time = if raw_time >= current_time {
                        raw_time
                    } else {
                        current_time
                    };

                    (callback.lock().unwrap())(&mut interleaved_data_buffer);

                    write_samples(
                        &buffer,
                        params.channels_count,
                        &interleaved_data_buffer,
                        &mut temp_samples,
                    );

                    create_buffer_source(
                        &audio_context_clone,
                        &buffer,
                        start_time,
                        &onended_closure_clone,
                    );

                    *time.write().unwrap() = start_time + buffer_duration_secs;
                })));

            // Run closures one after another to run the feed loop.
            window
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    onended_closure
                        .read()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .as_ref()
                        .unchecked_ref(),
                    offset_ms,
                )
                .map_err(convert_err)?;

            offset_ms += time_step_ms;
        }

        let _ = audio_context.resume().map_err(convert_err)?;

        Ok(Self { audio_context })
    }
}

impl Drop for WebAudioDevice {
    fn drop(&mut self) {
        let _ = self.audio_context.close().unwrap();
    }
}
