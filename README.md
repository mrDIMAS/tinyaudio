# TinyAudio

TinyAudio is a cross-platform audio output library. Its main goal is to provide unified access to
a default sound output device of your operating system as easy as possible, covering as many platforms
such as PC (Windows, Linux, macOS), Mobile Devices (Android, iOS), and WebAssembly.

## What this crate can do

The crate just takes the data you've prepared and sends it to a default operating system's sound output
device. It uses floating-point audio samples and converts them to the closest supported platform-dependent
format automatically. The crate guarantees that the intermediate data buffer will always be of requested size.
Use this crate, if you need to play your audio samples as easy as possible.

## What this crate cannot do

It does not load any sound formats, it doesn't apply any digital signal processing (DSP) techniques, it
doesn't do audio spatialization and so on. Also, the crate does not support device enumeration, device
selection, querying of supported formats, input capturing (i.e. from microphone).

## Supported platforms

| Windows | Linux | macOS | WebAssembly | Android | iOS |
|---------|-------|-------|-------------|---------|-----|
| ✅       | ✅     | ✅     | ✅           | ✅       | ✅   |

## How it works

The crate internally creates an audio output context and uses a user-defined callback to supply the device
with samples to play. The callback will be called periodically to generate new data; it will be called util
the device instance is "alive". In other words, this crate performs the simplest audio streaming.

## Android details

This crate uses `AAudio` for audio output on Android platform. `AAudio` is quite new API, which was added in ~2017
(in Android 8.1 Oreo). This means that you have to use `API Level 26+` to get the crate up and running. Also, you must
initialize an audio device only after your application has gained focus (`GainedFocus` event in `android-activity`
crate), otherwise device creation will fail. See `android-examples`
[directory](https://github.com/mrDIMAS/tinyaudio/tree/main/android-examples) for examples.

## WebAssembly details

Most of the web browsers nowadays require a "confirmation" action from a user (usually a button click or something
similar) to allow a web page to play an audio. This means that you must initialize an audio device _only_ after some
action on
a web page that runs your WebAssembly package. In the simplest scenario, it could be a simple button with a callback
that initializes an audio device. See `wasm-examples`
[directory](https://github.com/mrDIMAS/tinyaudio/tree/main/wasm-examples) for examples.

## Linux details

Do not forget to install the required development libraries, otherwise the crate won't compile:

```shell
sudo apt-get install libasound2-dev libudev-dev pkg-config
```

### Backends

Linux supports two audio "backends" - `ALSA` and `PulseAudio`. By default, this crate uses `ALSA`, but this can be
changed by specifying the `pulse` feature:

```toml
tinyaudio = { version = "2", default-features = false, features = ["pulse"] }
```

`PulseAudio` backend requires `libpulse-dev` to be installed:

```shell
sudo apt-get install libpulse-dev
```

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

## Comparison with alternatives

The closest alternative is `cpal` which is much more feature-rich, and has more complex API. Initialization of
`cpal` is quite verbose and could be confusing.
Compare [this example](https://github.com/RustAudio/cpal/blob/f43d36e55494993bbbde3299af0c53e5cdf4d4cf/examples/beep.rs)
from `cpal` with the example from the above code snippet. The next main difference is that `cpal` does not guarantee
that the size of the output buffer will be exactly the same as requested during the creation of audio stream, while
`TinyAudio` strictly guarantees this. Having a buffer of fixed size could be mandatory for some algorithms (such as
HRTF). That last main difference is fixed sample format - it is guaranteed to be `f32`. This simplifies a lot of
algorithms and has almost the same performance as with integer samples on relatively modern hardware.

Feature-parity with `cpal` is not a goal for this library, its main goal is to do one particular task, but do it as well
as possible.
