# TinyAudio example for iOS

A very simple iOS app that plays a sine-wave at launch from a Rust library using TinyAudio  


Requirements
--
- You need cargo-lipo to build the universal library:  
`cargo install cargo-lipo`

- The iOS project requires adding a link to the header file(s) from the rust project
- Add/link other libraries:
  - `AudioToolbox.framework`
  - `libresolv.tbd`
  - `libtinyaudioexample.a`

Building
--
Just build and launch the iOS project normally, the Rust library will be automatically rebuilt each time

Other details
--

The iOS project has 2 custom "Run Script Phases" in 'Build Phase' settings
- First: build static library from the rust project  
   `cargo lipo --xcode-integ --manifest-path $(PROJECT_DIR)../Rust-TinyAudioExample/Cargo.toml`

- *... normal compile / link / bundle steps ...*
   
- Second: delete static library in rust project  
   `rm -fv $(PROJECT_DIR)/../Rust-TinyAudioExample/target/universal/*/*.a`



Useful links
--
- https://mozilla.github.io/firefox-browser-architecture/experiments/2017-09-06-rust-on-ios.html
- https://github.com/TimNN/cargo-lipo?tab=readme-ov-file#xcode-integration
