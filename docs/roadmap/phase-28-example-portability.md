# Phase 28: Example Portability and Safety

**Goal**: Make all examples copyable outside the repo (no repo-relative paths in build.rs) and eliminate unnecessary unsafe code by pushing platform details into BSP crates.

**Status**: In Progress
**Priority**: Medium
**Depends on**: Phase 14 (BSP libraries) — complete, Phase 26 (typed API) — complete

## Problem Statement

### build.rs repo-root walking

8 embedded examples locate pre-built libraries by walking up the directory tree:

```rust
// examples/qemu/bsp-talker/build.rs (and 7 others)
let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
let repo_root = manifest_dir.parent().unwrap().parent().unwrap().parent().unwrap();
let zenoh_pico_lib = repo_root.join("build/qemu-zenoh-pico");
println!("cargo:rustc-link-search=native={}", zenoh_pico_lib.display());
```

This breaks if an example is copied out of the repo. It also assumes a fixed 3-level directory depth.

QEMU examples also reference a shared linker script via compile-time include:

```rust
File::create(out.join("memory.x")).unwrap()
    .write_all(include_bytes!("../../platform-integration/qemu-smoltcp-bridge/mps2-an385.x"))
    .unwrap();
```

### Unsafe code in examples

Native examples have zero unsafe — the API works well there. Unsafe concentrates in embedded examples:

| Pattern                       | Examples                       | Lines | Root cause                               |
|-------------------------------|--------------------------------|-------|------------------------------------------|
| `static mut` callback buffers | 4 listeners                    | ~20   | BSP doesn't provide safe message storage |
| Global socket table           | qemu-smoltcp-bridge            | ~80   | Reference impl exposes internals         |
| libc stubs                    | qemu-smoltcp-bridge            | ~250  | zenoh-pico needs libc on bare-metal      |
| DMA buffer `#[link_section]`  | 3 STM32F4 platform-integration | ~12   | Hardware memory layout                   |
| Zephyr FFI boundary           | 6 Zephyr examples              | ~6    | Inherent to C-Rust FFI                   |

The first three are fixable by moving platform concerns into BSP/support crates.

## Affected Files

### build.rs with repo-root walking (8 files)

**ARM QEMU** (find `build/qemu-zenoh-pico/libzenohpico.a`):
- `examples/qemu/rs-talker/build.rs`
- `examples/qemu/rs-listener/build.rs`
- `examples/qemu/bsp-talker/build.rs`
- `examples/qemu/bsp-listener/build.rs`

**ESP32** (find `build/esp32-zenoh-pico/libzenohpico.a`):
- `examples/esp32/bsp-talker/build.rs`
- `examples/esp32/bsp-listener/build.rs`
- `examples/esp32/qemu-talker/build.rs`
- `examples/esp32/qemu-listener/build.rs`

### build.rs that are fine (5 files)

These use `include_bytes!()` on local files — no repo-root needed:
- `examples/platform-integration/stm32f4-rtic/build.rs`
- `examples/platform-integration/stm32f4-polling/build.rs`
- `examples/platform-integration/stm32f4-smoltcp/build.rs`
- `examples/platform-integration/qemu-lan9118/build.rs`
- `examples/qemu/rs-test/build.rs`

### Unsafe code in examples

**`static mut` callback buffers** (should be safe):
- `examples/qemu/bsp-listener/src/main.rs` — `static mut LAST_VALUE: i32`
- `examples/qemu/rs-listener/src/main.rs` — `static mut LAST_VALUE: i32`
- `examples/esp32/bsp-listener/src/main.rs` — `static mut MSG_BUFFER`, `MSG_LEN`
- `examples/esp32/qemu-listener/src/main.rs` — `static mut LAST_VALUE`, `MSG_COUNT`

**Platform internals in example code** (should be in BSP/support crate):
- `examples/platform-integration/qemu-smoltcp-bridge/src/bridge.rs` — global socket table
- `examples/platform-integration/qemu-smoltcp-bridge/src/libc_stubs.rs` — C stdlib stubs
- `examples/platform-integration/qemu-smoltcp-bridge/src/clock.rs` — FFI time functions

**Hardware layout** (inherent, kept in platform-integration):
- `examples/platform-integration/stm32f4-rtic/src/main.rs` — `#[link_section = ".ethram"]`
- `examples/platform-integration/stm32f4-polling/src/main.rs` — same
- `examples/platform-integration/stm32f4-smoltcp/src/main.rs` — same

**Zephyr FFI** (inherent, minimal):
- `examples/zephyr/rs-{talker,listener,service-*,action-*}/src/lib.rs` — `#[unsafe(no_mangle)] extern "C" fn rust_main()`

## Work Items

### 28.1: Inline zenoh-pico build in zenoh-pico-shim-sys

**Status**: Complete
**Priority**: High — this is the main portability blocker

Move zenoh-pico cross-compilation from external shell scripts and example build.rs files into `zenoh-pico-shim-sys/build.rs`. For embedded + smoltcp targets, the build script compiles all zenoh-pico sources (core + platform + shim) using the `cc` crate. No pre-build step or environment variable needed.

**Approach**: `zenoh-pico-shim-sys/build.rs` detects embedded targets (`thumbv7m`, `thumbv7em`, `riscv32imc`) and builds zenoh-pico inline with `cc::Build`. This mirrors what `scripts/{qemu,esp32}/build-zenoh-pico.sh` did, but integrated into the cargo build. For RISC-V, a shadow `errno.h` avoids picolibc's TLS-based errno (which crashes on bare-metal ESP32-C3).

**Changes**:
- [x] `crates/zenoh-pico-shim-sys/build.rs` — `build_zenoh_pico_embedded()` compiles ~120 sources for embedded targets
- [x] Remove zenoh-pico link logic from all 8 example build.rs files (files deleted entirely)
- [x] Remove `ZENOH_PICO_LIB_DIR` env var from justfile, BSP build.rs files, and CLAUDE.md
- [x] Remove `build-zenoh-pico-arm` / `build-zenoh-pico-riscv` as build dependencies (recipes kept for manual use)
- [x] `crates/nano-ros-bsp-qemu/build.rs` — linker script only (no zenoh-pico linkage)
- [x] `crates/nano-ros-bsp-esp32{,-qemu}/build.rs` — deleted (no longer needed)

**Acceptance criteria**:
- `cargo build --release` in any QEMU/ESP32 example builds zenoh-pico automatically
- No environment variables, pre-build steps, or repo-root walking
- Examples can be copied to a standalone directory and build (given correct crate deps)

### 28.2: BSP crates own linker scripts

**Status**: Complete
**Priority**: High

The `mps2-an385.x` linker script is currently shared via `include_bytes!("../../platform-integration/qemu-smoltcp-bridge/mps2-an385.x")`. It should ship with the BSP crate.

**Changes**:
- [x] Copy `mps2-an385.x` into `crates/nano-ros-bsp-qemu/` (canonical location)
- [x] `crates/nano-ros-bsp-qemu/build.rs` — write linker script to `OUT_DIR` and emit `cargo:rustc-link-search`
- [x] Remove linker script handling from QEMU example build.rs files (build.rs files deleted entirely)
- [x] ESP32 BSP not applicable (esp-hal manages linker scripts)
- [x] `examples/platform-integration/qemu-smoltcp-bridge/mps2-an385.x` — kept as reference, no longer imported by other examples

**Acceptance criteria**:
- QEMU examples have no `include_bytes!` referencing paths outside their own directory
- Linker script is part of the BSP crate dependency chain

### 28.3: Safe message storage in BSP listener API

**Status**: Not Started
**Priority**: Medium

Replace `static mut LAST_VALUE` pattern in listener examples with safe abstractions provided by the BSP.

**Current pattern** (unsafe):
```rust
static mut LAST_VALUE: i32 = 0;

fn on_message(msg: &Int32) {
    unsafe { LAST_VALUE = msg.data; }
}

// In main loop:
let value = unsafe { LAST_VALUE };
```

**Target pattern** (safe):
```rust
// Option A: Atomic wrapper (works on platforms with atomic CAS)
static LAST_VALUE: AtomicI32 = AtomicI32::new(0);

fn on_message(msg: &Int32) {
    LAST_VALUE.store(msg.data, Ordering::Relaxed);
}

// Option B: BSP-provided message cell
let msg_cell = node.create_message_cell::<Int32>();
let _sub = node.create_subscription::<Int32>("/chatter", msg_cell.callback());
// In main loop:
if let Some(msg) = msg_cell.take() { ... }
```

**Changes**:
- [ ] Evaluate which approach fits each platform (atomics available on Cortex-M3+, not on ESP32-C3 riscv32imc without A extension)
- [ ] For QEMU ARM: use `AtomicI32` directly (Cortex-M3 supports atomic loads/stores)
- [ ] For ESP32-C3: provide `critical_section`-based `Cell` wrapper in BSP
- [ ] Update 4 listener examples to use safe pattern
- [ ] Document the pattern in BSP crate docs

**Acceptance criteria**:
- Zero `static mut` in BSP examples (qemu/bsp-listener, qemu/rs-listener)
- ESP32 listeners use safe wrappers appropriate for the platform

### 28.4: Move libc stubs to support crate

**Status**: Not Started
**Priority**: Low

`examples/platform-integration/qemu-smoltcp-bridge/src/libc_stubs.rs` (~250 lines) provides minimal C stdlib functions required by zenoh-pico on bare-metal. Every bare-metal example that links zenoh-pico needs these. They should not live in an example.

**Approach**: Create `crates/nano-ros-libc-stubs/` or fold into `zenoh-pico-shim-sys` build for bare-metal targets.

**Changes**:
- [ ] Decide location: new crate vs. conditional compilation in `zenoh-pico-shim-sys`
- [ ] Move `libc_stubs.rs` content (strlen, memcpy, memmove, memset, memcmp, memchr, strcmp, strncmp, strncpy, strtoul, snprintf stubs)
- [ ] Move `clock.rs` FFI functions if they are also platform infrastructure
- [ ] Update `qemu-smoltcp-bridge` to depend on the new location
- [ ] Update BSP crates to pull in stubs automatically

**Acceptance criteria**:
- No libc stub implementations in `examples/` directory
- BSP crate dependency chain provides all required C symbols

### 28.5: Document platform-integration as reference-only

**Status**: Not Started
**Priority**: Low

The `examples/platform-integration/` directory contains low-level reference implementations (smoltcp bridge, STM32F4 networking). These are not meant to be copied — they exist to show how BSPs are built internally.

**Changes**:
- [ ] Update `examples/platform-integration/README.md` to clearly state these are reference implementations for BSP developers, not application examples
- [ ] Note that the unsafe code (DMA buffers, socket tables) is intentional and expected at this level
- [ ] Cross-reference the BSP examples (`qemu/bsp-*`, `stm32f4/bsp-*`) as the recommended starting point

## Non-Goals

- **Zephyr FFI boundary**: `#[unsafe(no_mangle)] extern "C" fn rust_main()` is inherent to the Zephyr C-Rust integration. Cannot be removed without changing the Zephyr module architecture.
- **DMA buffer `#[link_section]`**: Hardware memory layout annotations in platform-integration examples are correct uses of unsafe. These are reference implementations, not user-facing examples.
- **Full libc replacement**: We provide stubs sufficient for zenoh-pico, not a complete bare-metal libc.

## Dependencies

```
28.1 (zenoh-pico linkage) ──┬──► 28.4 (libc stubs)
                            │
28.2 (linker scripts) ──────┤
                            │
28.3 (safe message storage) ┘    28.5 (docs) — independent
```

28.1 and 28.2 can proceed in parallel. 28.3 is independent. 28.4 builds on the BSP changes from 28.1. 28.5 is standalone documentation.

## Success Metrics

| Metric | Before | After |
|--------|--------|-------|
| Examples with repo-root walking in build.rs | 8 | 0 |
| `static mut` in BSP examples | 4 files | 0 |
| libc stubs in examples/ | 250 lines | 0 |
| Examples copyable outside repo | 0 | All BSP examples |
| Unsafe blocks in BSP examples | ~20 lines | ~6 (Zephyr FFI only) |
