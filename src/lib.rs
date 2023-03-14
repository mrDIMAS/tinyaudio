#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

use std::error::Error;

mod aaudio;
mod alsa;
mod coreaudio;
mod directsound;
mod web;

#[doc(hidden)]
pub mod prelude {
    pub use super::{run_output_device, BaseAudioOutputDevice, OutputDeviceParameters};
}

/// Parameters of an output device.
#[derive(Copy, Clone)]
pub struct OutputDeviceParameters {
    /// Sample rate of your audio data.
    pub sample_rate: usize,

    /// Desired amount of audio channels. Must be at least one. Typical values: 1 - mono, 2 - stereo, etc.
    /// The data provided by the call back is _interleaved_, which means that if you have two channels then
    /// the sample layout will be like so: `LRLRLR..`, where `L` - a sample of left channel, and `R` a sample
    /// of right channel.
    pub channels_count: usize,

    /// Amount of samples per each channel. Allows you to tweak audio latency, the more the value the more
    /// latency will be and vice versa. Keep in mind, that your data callback must be able to render the
    /// samples while previous portion of data is being played, otherwise you'll get a glitchy audio.
    ///
    /// If you need to get a specific length in **seconds**, then you need to use sampling rate to calculate
    /// the required amount of samples per channel: `channel_sample_count = sample_rate * time_in_seconds`.
    ///
    /// The crate guarantees, that the intermediate buffer size will match the requested value.
    pub channel_sample_count: usize,
}

/// A base trait of a platform-dependent audio output device.
pub trait BaseAudioOutputDevice: Send {}

impl BaseAudioOutputDevice for () {}

trait AudioOutputDevice: BaseAudioOutputDevice {
    fn new<C>(params: OutputDeviceParameters, data_callback: C) -> Result<Self, Box<dyn Error>>
    where
        C: FnMut(&mut [f32]) + Send + 'static,
        Self: Sized;
}

/// Creates a new output device that uses default audio output device of your operating system to play the
/// samples produced by the specified `data_callback`. The callback will be called periodically to generate
/// another portion of samples.
pub fn run_output_device<C>(
    params: OutputDeviceParameters,
    data_callback: C,
) -> Result<Box<dyn BaseAudioOutputDevice>, Box<dyn Error>>
where
    C: FnMut(&mut [f32]) + Send + 'static,
{
    #[cfg(target_os = "windows")]
    {
        return Ok(Box::new(directsound::DirectSoundDevice::new(
            params,
            data_callback,
        )?));
    }

    #[cfg(target_os = "android")]
    {
        return Ok(Box::new(aaudio::AAudioOutputDevice::new(
            params,
            data_callback,
        )?));
    }

    #[cfg(target_os = "linux")]
    {
        return Ok(Box::new(alsa::AlsaSoundDevice::new(params, data_callback)?));
    }

    #[cfg(all(target_os = "unknown", target_arch = "wasm32"))]
    {
        return Ok(Box::new(web::WebAudioDevice::new(params, data_callback)?));
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        return Ok(Box::new(coreaudio::CoreaudioSoundDevice::new(
            params,
            data_callback,
        )?));
    }

    #[cfg(not(any(
        target_os = "windows",
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        all(target_os = "unknown", target_arch = "wasm32")
    )))]
    {
        Err("Platform is not supported".to_string().into())
    }
}
