//! Android output device via `AAudio`

#![cfg(target_os = "android")]

use crate::{AudioOutputDevice, BaseAudioOutputDevice, OutputDeviceParameters};
use aaudio::{AAudioStream, AAudioStreamBuilder, CallbackResult, Direction, Format};
use std::{collections::VecDeque, error::Error};

pub struct AAudioOutputDevice {
    stream: AAudioStream,
}

impl BaseAudioOutputDevice for AAudioOutputDevice {}

fn convert_err(err: aaudio::Error) -> Box<dyn Error> {
    format!("{:?}", err).into()
}

impl AudioOutputDevice for AAudioOutputDevice {
    fn new<C>(params: OutputDeviceParameters, mut data_callback: C) -> Result<Self, Box<dyn Error>>
    where
        C: FnMut(&mut [f32]) + Send + 'static,
        Self: Sized,
    {
        let mut stream = AAudioStreamBuilder::new()
            .map_err(convert_err)?
            .set_buffer_capacity_in_frames(params.channel_sample_count as i32)
            .set_channel_count(params.channels_count as i32)
            .set_format(Format::F32)
            .set_direction(Direction::Output)
            .set_callbacks(
                {
                    let mut samples_queue = VecDeque::<f32>::new();
                    move |_, data, num_samples_per_channel| {
                        // Render another portion of data if needed.
                        while num_samples_per_channel as usize
                            > samples_queue.len() / params.channels_count
                        {
                            let write_pos = samples_queue.len();

                            let new_len = samples_queue.len()
                                + params.channel_sample_count * params.channels_count;

                            while samples_queue.len() < new_len {
                                samples_queue.push_back(0.0);
                            }

                            samples_queue.make_contiguous();

                            let (left, _) = samples_queue.as_mut_slices();
                            let dest_data = &mut left[write_pos..];
                            debug_assert_eq!(
                                dest_data.len(),
                                params.channel_sample_count * params.channels_count
                            );
                            data_callback(dest_data);
                        }

                        let output_data = unsafe {
                            std::slice::from_raw_parts_mut::<f32>(
                                data.as_mut_ptr() as *mut f32,
                                num_samples_per_channel as usize * params.channels_count,
                            )
                        };

                        for sample in output_data {
                            *sample = samples_queue.pop_back().expect("Queue underflow!")
                        }

                        CallbackResult::Continue
                    }
                },
                |_, _| {},
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
