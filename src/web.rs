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

#[cfg(not(target_feature = "atomics"))]
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

#[cfg(target_feature = "atomics")]
mod atomics {
    use crate::wasm_bindgen;
    use js_sys::wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use web_sys::AudioBuffer;

    // Create custom bindings to AudioBuffer `copyToChannel` method, that does not use generated
    // bindings to Rust arrays and just uses plain JS array.
    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_name = AudioBuffer)]
        type ArrayAudioBuffer;

        # [wasm_bindgen(catch, method, structural, js_class = "AudioBuffer", js_name = copyToChannel)]
        pub fn copy_to_channel(
            this: &ArrayAudioBuffer,
            source: &js_sys::Float32Array,
            channel_number: i32,
        ) -> Result<(), JsValue>;
    }

    pub fn write_samples(
        buffer: &AudioBuffer,
        channels_count: usize,
        interleaved_data_buffer: &[f32],
        temp_samples: &mut Vec<f32>,
        temporary_channel_array_view: &js_sys::Float32Array,
    ) {
        for channel_index in 0..channels_count {
            // Copy channel samples from the interleaved buffer into the temporary one.
            temp_samples.clear();
            for samples in interleaved_data_buffer.chunks(channels_count) {
                temp_samples.push(samples[channel_index]);
            }

            // Do another clone to temporary JS buffer.
            temporary_channel_array_view.copy_from(temp_samples);

            // Copy samples from this temporary buffer to the channel buffer.
            buffer
                .unchecked_ref::<ArrayAudioBuffer>()
                .copy_to_channel(&temporary_channel_array_view, channel_index as i32)
                .unwrap();
        }
    }

    pub fn make_temp_js_buffer(channel_sample_count: usize) -> js_sys::Float32Array {
        let byte_count = (size_of::<f32>() * channel_sample_count) as u32;
        let temporary_channel_array = js_sys::ArrayBuffer::new(byte_count);
        js_sys::Float32Array::new(&temporary_channel_array)
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

            #[cfg(target_feature = "atomics")]
            let temp_js_samples = atomics::make_temp_js_buffer(params.channel_sample_count);

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

                    #[cfg(not(target_feature = "atomics"))]
                    {
                        write_samples(
                            &buffer,
                            params.channels_count,
                            &interleaved_data_buffer,
                            &mut temp_samples,
                        );
                    }

                    #[cfg(target_feature = "atomics")]
                    {
                        atomics::write_samples(
                            &buffer,
                            params.channels_count,
                            &interleaved_data_buffer,
                            &mut temp_samples,
                            &temp_js_samples,
                        )
                    }

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
