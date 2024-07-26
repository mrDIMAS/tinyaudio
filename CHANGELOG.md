# Unpublished

- Added opaque handle to sound output device.
- Ability to close audio output device without dropping it (via `close` method). Useful on platforms with garbage
  collection (such as WebAssembly).

# 0.1.4

- Fixed more compilation issues on 32-bit targets on Linux.

# 0.1.3

- Fixed audio stutters on Android.
- Correctly pass sample rate to output device config on Android.
- Print errors to `stderr` on Android.

# 0.1.2

- Fixed crash on some Linux distros due to the use of `snd_pcm_hw_params_set_period_size`

# 0.1.1

- Fixed compilation issues on 32-bit targets on Linux

# 0.1.0

- First public release