#![cfg(target_os = "android")]

use android_activity::{AndroidApp, MainEvent, PollEvent};
use tinyaudio::prelude::*;

fn play_sine_wave() -> OutputDevice {
    let params = OutputDeviceParameters {
        channels_count: 2,
        sample_rate: 44100,
        channel_sample_count: 4410,
    };

    run_output_device(params, {
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
    .unwrap()
}

#[no_mangle]
fn android_main(app: AndroidApp) {
    let mut device = None;

    loop {
        app.poll_events(
            Some(std::time::Duration::from_millis(100)),
            |event| match event {
                PollEvent::Main(main_event) => match main_event {
                    MainEvent::GainedFocus => {
                        device = Some(play_sine_wave());
                    }
                    MainEvent::Destroy => {
                        return;
                    }
                    _ => {}
                },
                _ => {}
            },
        );
    }
}
