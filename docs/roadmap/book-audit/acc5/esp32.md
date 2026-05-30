BLOCKERS
None. QEMU happy path runs end-to-end — talker emits the documented `Declaring publisher on /chatter (std_msgs/Int32)` / `Publisher declared` / `Published: 0..97` lines (98 publishes in a 30 s window, via openeth slirp to zenohd on 127.0.0.1:7454).

FRICTION (fixed in `3b17fcc66`)
1. `just esp32 build` doc-vs-reality: doc comment said "builds for the QEMU board crate" but the recipe builds the real-hardware fixtures under `examples/esp32/rust/`. Actual QEMU build is `just esp32 build-qemu` (the dep of `just esp32 talker`). Inline comment fixed.
2. "Prebuilt esp-hal toolchain" wording was misleading — `nros setup esp32 --rmw zenoh` resolves only zenohd + zenoh-pico + mbedtls. No esp-hal toolchain is installed; esp-hal is a Cargo dep. The only cross-toolchain is `rustup target add riscv32imc-unknown-none-elf`. Rewrote the Setup paragraph + named the rustup target add step.
3. Missing codegen step: layout showed `examples/esp32/rust/talker/generated/` but the doc never told the reader where it comes from. Build silently requires it (`.cargo/config.toml` patches `std_msgs` / `builtin_interfaces` to that dir). Added a one-line note that the example's `build.rs` invokes `nros generate-rust` automatically on first build.
4. Timing claims optimistic: "QEMU ESP32: ~15 seconds" only with a fully-warm cache. `just esp32 talker` re-runs `build-qemu` every invocation (adds ~25 s on top). Acknowledged in the readiness block.

CLARITY
- CLEAR otherwise.

MISSING STEPS
- None now (codegen note added).

WORKS
- `nros setup esp32 --rmw zenoh` (exit 0).
- `cargo build --release` for real-hw talker (15.87 s, exit 0).
- `just esp32 build-qemu` (exit 0, bins in `build/esp32-qemu/`).
- `just esp32 zenohd` (port 7454 listening, with PATH set per doc).
- QEMU talker on `qemu-system-riscv32 -M esp32c3 ... openeth` (98 Published lines / 30 s).
- ESP32-S3 (Xtensa) carve-out agrees with the in-tree board crate (RISC-V only).

Acceptance bar (0 BLOCKERS) MET.

LAST COMMAND: timeout 30 just esp32 talker
LAST EXIT CODE: 124 (timeout-killed; intentional)
