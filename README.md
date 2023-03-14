# TinyAudio

TinyAudio is a cross-platform audio output library. Its main goal to provide unified access to
a default sound output device of your operating system as easy as possible, covering as many platforms
such as PC (Windows, Linux, macOS), Mobile Devices (Android, iOS), and WebAssembly.

## What this crate can do

The crate just takes the data you've prepared and sends it to a default operating system's sound output
device. It uses floating-point audio samples and converts them to the closest supported platform-dependent
format automatically. The crate guarantees, that the intermediate data buffer will always be of requested size.
Use this crate, if you need to play your audio samples as easy as possible.

## What this crate cannot do

It does not load any sound formats, it doesn't apply any digital signal processing (DSP) techniques, it
doesn't do audio spatialization and so on. Also, the crate does not support device enumeration, device
selection, querying of supported formats, input capturing (i.e. from microphone).

## Supported platforms

| Windows | Linux | macOS | WebAssembly | Android | iOS |
|---------|-------|-------|-------------|---------|-----|
| ✅       | ✅     | ✅    | ✅           | ✅       | ✅  |

## How it works

The crate internally creates an audio output context and uses a user-defined callback to supply the device
with samples to play. The callback will be called periodically to generate new data; it will be called util
the device instance is "alive". In other words this crate performs the simplest audio streaming.

## Android details

This crate uses `AAudio` for audio output on Android platform. `AAudio` is quite new API, which was added in ~2017 
(in Android 8.1 Oreo). This means that you have to use `API Level 26+` to get the crate up and running. Also, you must
initialize an audio device only after your application has gained focus (`GainedFocus` event in `android-activity` crate),
otherwise device creation will fail. See `android-examples` 
[directory](https://github.com/mrDIMAS/tinyaudio/tree/main/android-examples) for examples. 

## WebAssembly details

Most of the web browsers nowadays requires a "confirmation" action from a user (usually a button click or something similar) to 
allow a web page to play an audio. This means that you must initialize an audio device _only_ after some action on
a web page that runs your WebAssembly package. In the simplest scenario it could be a simple button with a callback
that initializes an audio device. See `wasm-examples` [directory](https://github.com/mrDIMAS/tinyaudio/tree/main/android-examples)
for examples.

## Examples

The crate is very easy to use, here's a few examples that will help you to start using it right away.

### Initialization

The simplest possible example that shows how to initialize an output device.

```rust,no_run
use tinyaudio::prelude::*;

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
```

### Playing a sine wave

A simple example that plays a sine wave of 440 Hz looks like so:

```rust,no_run
# use tinyaudio::prelude::*;
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
```