# Text assembler evaluation

This standalone crate runs fixed, generated text fixtures and reports bytes
and diagnostics. It never executes the result or opens a document.
[ADR 0006](../../docs/adr/0006-text-assembler-selection.md)

From the repository root with Rust 1.90.0:

```sh
cargo test --locked --manifest-path experiments/assembler/Cargo.toml --features compare-asm-rs
cargo run --locked --manifest-path experiments/assembler/Cargo.toml --features compare-asm-rs
python3 experiments/assembler/compare-llvm.py /path/to/llvm/bin
```

The LLVM check uses `llvm-mc`, `llvm-objcopy`, and `llvm-readobj`.
To link and exercise the WebAssembly version with Node:

```sh
rustup target add --toolchain 1.90.0 wasm32-unknown-unknown
CARGO_TARGET_DIR=target/assembler-probe cargo build --locked \
  --manifest-path experiments/assembler/Cargo.toml --lib --release \
  --features compare-asm-rs --target wasm32-unknown-unknown
node experiments/assembler/check-wasm.mjs \
  target/assembler-probe/wasm32-unknown-unknown/release/hiewbie_assembler_probe.wasm
```

For compile-only routes, install the standard library for the pinned
toolchain and run:

```sh
cargo check --locked --manifest-path experiments/assembler/Cargo.toml \
  --features compare-asm-rs --target "$BUILD_TARGET"
```
