## Build instructions

1. Make sure you have `wasm32-unknown-unknown` target installed in rustup (if not, do: `rustup target add wasm32-unknown-unknown`)
2. Make sure you have `wasm-pack` installed (if not, do: `cargo install wasm-pack`)
3. To build the example, do: `wasm-pack build --target web --release`
4. Make sure you have `basic-http-server` installed (if not, do: `cargo install basic-http-server`).
5. Execute `basic-http-server` in `wasm-examples` directory.

If everything has succeeded, open a web browser at http://localhost:4000/, click "Start" button, and you should hear
a sine wave.