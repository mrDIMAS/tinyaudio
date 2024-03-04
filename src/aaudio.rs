//! Android output device via `AAudio`

#![cfg(target_os = "android")]

use crate::{AudioOutputDevice, BaseAudioOutputDevice, OutputDeviceParameters};
use aaudio::{
    AAudioStream, AAudioStreamBuilder, CallbackResult, Direction, Format, PerformanceMode,
};
use std::error::Error;

pub struct AAudioOutputDevice {
    stream: AAudioStream,
}

impl BaseAudioOutputDevice for AAudioOutputDevice {}

unsafe impl Send for AAudioOutputDevice {}

fn convert_err(err: aaudio::Error) -> Box<dyn Error> {
    format!("{:?}", err).into()
}

impl AudioOutputDevice for AAudioOutputDevice {
    fn new<C>(params: OutputDeviceParameters, mut data_callback: C) -> Result<Self, Box<dyn Error>>
    where
        C: FnMut(&mut [f32]) + Send + 'static,
        Self: Sized,
    {
        let frame_count = params.channel_sample_count as i32;
        let mut stream = AAudioStreamBuilder::new()
            .map_err(convert_err)?
            // Ensure double buffering is possible.
            .set_buffer_capacity_in_frames(2 * frame_count)
            .set_channel_count(params.channels_count as i32)
            .set_format(Format::F32)
            .set_sample_rate(params.sample_rate as i32)
            .set_direction(Direction::Output)
            .set_performance_mode(PerformanceMode::LowLatency)
            // Force the AAudio to give the buffer of fixed size.
            .set_frames_per_data_callback(frame_count)
            .set_callbacks(
                move |_, data, num_frames| {
                    let output_data = unsafe {
                        std::slice::from_raw_parts_mut::<f32>(
                            data.as_mut_ptr() as *mut f32,
                            num_frames as usize * params.channels_count,
                        )
                    };

                    data_callback(output_data);

                    CallbackResult::Continue
                },
                |_, error| eprintln!("AAudio: an error has occurred - {:?}", error),
            )
            .open_stream()
            .map_err(convert_err)?;

        stream.request_start().map_err(convert_err)?;

        Ok(Self { stream })
    }
}

impl Drop for AAudioOutputDevice {
    fn drop(&mut self) {
        self.stream
            .release()
            .expect("Failed to release the stream!")
    }
}
