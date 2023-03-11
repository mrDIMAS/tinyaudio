//! Basic initialization example. This example does not produce any audible samples to just show
//! the initialization procedure.

use tinyaudio::prelude::*;

fn main() {
    let _device = run_output_device(
        OutputDeviceParameters {
            channels_count: 2,
            sample_rate: 44100,
            channel_sample_count: 4410,
        },
        move |_| {
            // Output silence
        },
    )
    .unwrap();

    std::thread::sleep(std::time::Duration::from_secs(1));
}
