use tinyaudio::prelude::*;
static mut DEVICE_HANDLE: Option<Box<dyn BaseAudioOutputDevice>> = None;

#[no_mangle]
pub extern "C" fn create_audio_device() -> i32 {
    let params = OutputDeviceParameters {
        channels_count: 2,
        sample_rate: 44100,
        channel_sample_count: 4410,
    };

    let device_result = run_output_device(params, {
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
    });
    match device_result {
        Ok(device) => {
            unsafe { DEVICE_HANDLE = Some(device); }
            1
        }
        Err(_) => {
            -1
        }
    }
}

#[no_mangle]
pub extern "C" fn is_audio_initialized() -> i32 {
    unsafe {
        match DEVICE_HANDLE.is_some() {
            true => { 1 }
            false => { 0 }
        }
    }
}

#[no_mangle]
pub extern "C" fn destroy_audio_device() {
    unsafe {
        DEVICE_HANDLE = None;
    }
}