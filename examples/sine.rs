//! This example initializes a default sound output device of your operating system and plays a sine wave
//! of 440 Hz for 5 seconds.

use tinyaudio::prelude::*;

fn main() {
    let params = OutputDeviceParameters {
        channels_count: 2,
        sample_rate: 44100,
        channel_sample_count: 4410,
    };

    let _device = run_output_device(params, {
        let mut clock = 0f32;
        move |data| {
            for samples in data.chunks_mut(params.channels_count) {
                clock = (clock + 1.0) % params.sample_rate as f32;
                let value =
                    (clock * 440.0 * 2.0 * std::f32::consts::PI / params.sample_rate as f32).sin();
                for sample in samples {
                    *sample = value;
                }
            }
        }
    })
    .unwrap();

    std::thread::sleep(std::time::Duration::from_secs(5));
}
