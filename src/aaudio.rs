//! Android output device via `AAudio`

#![cfg(target_os = "android")]

use crate::{AudioOutputDevice, BaseAudioOutputDevice, OutputDeviceParameters};
use ndk::audio::{
    AudioStream, AudioStreamBuilder, AudioCallbackResult, AudioDirection, AudioFormat, AudioPerformanceMode,
    AudioError,
};
use std::error::Error;

pub struct AAudioOutputDevice {
    stream: AudioStream,
}

impl BaseAudioOutputDevice for AAudioOutputDevice {}

unsafe impl Send for AAudioOutputDevice {}

fn convert_err(err: AudioError) -> Box<dyn Error> {
    format!("{:?}", err).into()
}

impl AudioOutputDevice for AAudioOutputDevice {
    fn new<C>(params: OutputDeviceParameters, mut data_callback: C) -> Result<Self, Box<dyn Error>>
    where
        C: FnMut(&mut [f32]) + Send + 'static,
        Self: Sized,
    {
        let frame_count = params.channel_sample_count as i32;
        let mut stream = AudioStreamBuilder::new()
            .map_err(convert_err)?
            // Ensure double buffering is possible.
            .buffer_capacity_in_frames(2 * frame_count)
            .channel_count(params.channels_count as i32)
            .format(AudioFormat::PCM_Float)
            .sample_rate(params.sample_rate as i32)
            .direction(AudioDirection::Output)
            .performance_mode(AudioPerformanceMode::LowLatency)
            // Force the AAudio to give the buffer of fixed size.
            .frames_per_data_callback(frame_count)
            .data_callback(
                Box::new(move |_, data, num_frames| {
                    let output_data = unsafe {
                        std::slice::from_raw_parts_mut::<f32>(
                            data as *mut f32,
                            num_frames as usize * params.channels_count,
                        )
                    };

                    data_callback(output_data);

                    AudioCallbackResult::Continue
                })
            )
            .error_callback(Box::new(|_, error| eprintln!("AAudio: an error has occurred - {:?}", error)))
            .open_stream()
            .map_err(convert_err)?;

        stream.request_start().map_err(convert_err)?;

        Ok(Self { stream })
    }
}
