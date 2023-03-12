## Build instructions

1. Make sure you have suitable toolchain installed, for example `armv7-linux-androideabi` (if not, do: 
`rustup target add armv7-linux-androideabi`)
2. Make sure you have NDK installed (if not - follow 
[these instructions](https://developer.android.com/studio/projects/install-ndk))
3. Make sure you have all environment variables prepared (if not - follow 
[these instructions](https://developer.android.com/studio/command-line/variables))
4. Make sure you have `cargo-apk` installed, (if not - do `cargo install cargo-apk`)
5. Plug-in your device via USB or run one on Android Emulator
6. `cargo-apk apk run --target=armv7-linux-androideabi`
7. A test app should run and you should hear a sine wave that will last for 5 seconds.