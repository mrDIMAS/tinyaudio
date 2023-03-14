#![cfg(all(target_os = "unknown", target_arch = "wasm32"))]

use crate::utils::set_panic_hook;
use tinyaudio::prelude::*;
use wasm_bindgen::prelude::*;

mod utils;

#[wasm_bindgen]
pub fn play_sine_wave() {
    set_panic_hook();

    let params = OutputDeviceParameters {
        channels_count: 2,
        sample_rate: 44100,
        channel_sample_count: 4410,
    };

    let device = run_output_device(params, {
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

    Box::leak(device);
}
