#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

use std::error::Error;

#[cfg(all(target_os = "unknown", target_arch = "wasm32"))]
use wasm_bindgen::prelude::wasm_bindgen;

mod aaudio;
mod alsa;
mod coreaudio;
mod directsound;
mod pulse;
mod web;

#[doc(hidden)]
pub mod prelude {
    pub use super::{run_output_device, OutputDevice, OutputDeviceParameters};
}

/// Parameters of an output device.
#[derive(Copy, Clone)]
pub struct OutputDeviceParameters {
    /// Sample rate of your audio data. Typical values are: 11025 Hz, 22050 Hz, 44100 Hz (default), 48000 Hz,
    /// 96000 Hz.
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

trait BaseAudioOutputDevice: Send + 'static {}

impl BaseAudioOutputDevice for () {}

trait AudioOutputDevice: BaseAudioOutputDevice {
    fn new<C>(params: OutputDeviceParameters, data_callback: C) -> Result<Self, Box<dyn Error>>
    where
        C: FnMut(&mut [f32]) + Send + 'static,
        Self: Sized;
}

/// An opaque "handle" to platform-dependent audio output device.
#[cfg_attr(all(target_os = "unknown", target_arch = "wasm32"), wasm_bindgen)]
pub struct OutputDevice {
    device: Option<Box<dyn BaseAudioOutputDevice>>,
}

impl OutputDevice {
    fn new<D: BaseAudioOutputDevice>(device: D) -> Self {
        Self {
            device: Some(Box::new(device)),
        }
    }
}

#[cfg_attr(all(target_os = "unknown", target_arch = "wasm32"), wasm_bindgen)]
impl OutputDevice {
    /// Closes the output device and release all system resources occupied by it. Any calls of this
    /// method after the device was closed does nothing.
    pub fn close(&mut self) {
        self.device.take();
    }
}

/// Creates a new output device that uses default audio output device of your operating system to play the
/// samples produced by the specified `data_callback`. The callback will be called periodically to generate
/// another portion of samples.
///
/// ## Examples
///
/// The following examples plays a 440 Hz sine wave for 5 seconds.
///
/// ```rust,no_run
/// # use tinyaudio::prelude::*;
/// let params = OutputDeviceParameters {
///     channels_count: 2,
///     sample_rate: 44100,
///     channel_sample_count: 4410,
/// };
///
/// let _device = run_output_device(params, {
///     let mut clock = 0f32;
///     move |data| {
///         for samples in data.chunks_mut(params.channels_count) {
///             clock = (clock + 1.0) % params.sample_rate as f32;
///             let value =
///                 (clock * 440.0 * 2.0 * std::f32::consts::PI / params.sample_rate as f32).sin();
///             for sample in samples {
///                 *sample = value;
///             }
///         }
///     }
/// })
/// .unwrap();
///
/// std::thread::sleep(std::time::Duration::from_secs(5));
/// ```
#[allow(clippy::needless_return)]
pub fn run_output_device<C>(
    params: OutputDeviceParameters,
    data_callback: C,
) -> Result<OutputDevice, Box<dyn Error>>
where
    C: FnMut(&mut [f32]) + Send + 'static,
{
    #[cfg(target_os = "windows")]
    {
        return Ok(OutputDevice::new(directsound::DirectSoundDevice::new(
            params,
            data_callback,
        )?));
    }

    #[cfg(target_os = "android")]
    {
        return Ok(OutputDevice::new(aaudio::AAudioOutputDevice::new(
            params,
            data_callback,
        )?));
    }

    #[cfg(target_os = "linux")]
    {
        #[cfg(feature = "alsa")]
        {
            return Ok(OutputDevice::new(alsa::AlsaSoundDevice::new(
                params,
                data_callback,
            )?));
        }

        #[cfg(all(feature = "pulse", not(feature = "alsa")))]
        {
            return Ok(OutputDevice::new(pulse::PulseSoundDevice::new(
                params,
                data_callback,
            )?));
        }

        #[cfg(all(not(feature = "alsa"), not(feature = "pulse")))]
        {
            compile_error!("Select \"alsa\" or \"pulse\" feature to use an audio device on Linux")
        }
    }

    #[cfg(all(target_os = "unknown", target_arch = "wasm32"))]
    {
        return Ok(OutputDevice::new(web::WebAudioDevice::new(
            params,
            data_callback,
        )?));
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        return Ok(OutputDevice::new(coreaudio::CoreaudioSoundDevice::new(
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
