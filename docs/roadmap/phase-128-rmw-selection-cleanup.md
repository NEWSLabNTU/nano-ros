# Phase 128 — RMW Selection Cleanup (Compile-Time, Manifest-Driven)

Date: 2026-05-16
Goal: Remove RMW/platform names from user code and from core libraries.
  Selection is determined at compile/link time by the user's manifest
  (Cargo `[dependencies]` or CMake `target_link_libraries`), with an
  optional `NROS_RMW` environment override for the single-backend case.
Status: planning
Priority: high (blocks future backend additions; pays off PR ergonomics)
Depends on: phase-104 (umbrella decoupling), phase-115 (cffi),
  phase-117 (cyclonedds), phase-121 (platform C-port canonicalization)

## Overview

Today nano-ros selects an RMW backend three different ways depending on
language and target:

- Rust no_std: explicit `nros_rmw_<name>::register()` in `main` + cargo
  features `rmw-<name>-cffi`, `platform-<rtos>`, `link-<transport>` on
  every dep.
- Rust POSIX: `.init_array` ctor inside `nros-rmw-<name>` auto-registers,
  but user still names backend in `Cargo.toml` features.
- C/C++: CMake `NANO_ROS_RMW=<name>` selects from a prebuilt staticlib
  matrix `libnros_c_<rmw>_<platform>.a`, and `nros::init()` has an
  `#ifdef NROS_RMW_<NAME>` chain that calls the backend register fn.

The three paths all duplicate the same axis (`<name>` ∈ {zenoh, dds,
xrce, cyclonedds, uorb}) at three layers and force core libraries to
carry backend-specific `#ifdef` blocks.

Phase 128 collapses selection into one mechanism that works the same
way on every target and in every language:

1. **Linker section discovery.** Each `nros-rmw-<name>` crate / static
   lib emits exactly one entry into a well-known linker section
   (`.nros_rmw_init`) at link time. The runtime walks the section on
   first `Executor::open` / `nros::init` and calls each entry's
   `nros_rmw_cffi_register_named` function. Same code path on POSIX,
   bare-metal, and RTOS targets. No dynamic loading.
2. **No `register()` calls in user code.** Manifest dep IS the
   selection. Source files never name a backend.
3. **No `#ifdef NROS_RMW_*` in core.** `nros::init` / `nros_init` /
   `Executor::open` become RMW-blind; the section walker handles
   registration.
4. **Optional `NROS_RMW=<name>` env var.** Mirrors ROS 2's
   `RMW_IMPLEMENTATION`. Only consulted when more than one backend is
   linked; resolves the ambiguity at runtime without rebuild.
5. **Bridge path keeps names explicit.** When the binary deliberately
   links multiple backends for cross-RMW bridging, the user calls
   `Executor::open_multi` + `create_node_on(node, "<rmw>")` and names
   are first-class arguments. Common single-backend code stays
   nameless.

## Architecture

```
                ┌─────────────────────────────────────┐
                │   user manifest (Cargo / CMake)     │
                │   selects backend(s) + platform     │
                └─────────────────┬───────────────────┘
                                  │
                          link time only
                                  │
       ┌──────────────────────────▼──────────────────────────┐
       │  linker section .nros_rmw_init                      │
       │    one fn-ptr entry per nros-rmw-<name>             │
       │  linker section .nros_platform_init                 │
       │    one fn-ptr entry per nros-platform-<name>        │
       └──────────────────────────┬──────────────────────────┘
                                  │
                  runtime walks once on first init
                                  │
       ┌──────────────────────────▼──────────────────────────┐
       │  nros-rmw-cffi  named registry                       │
       │  nros-platform-cffi  symbol publisher                │
       └──────────────────────────┬──────────────────────────┘
                                  │
                          selection at open:
                                  │
       ┌──────────────────────────▼──────────────────────────┐
       │  Executor::open(&cfg)            single-backend path │
       │    1. $NROS_RMW   → registry[$NROS_RMW]              │
       │    2. exactly 1   → that one                         │
       │    3. >1 + no env → error "ambiguous; set NROS_RMW"  │
       │    4. 0 linked    → error "no backend linked"        │
       │                                                      │
       │  Executor::open_multi(&[SessionSpec])   bridge path  │
       │    explicit per-spec rmw name lookup                 │
       └──────────────────────────────────────────────────────┘
```

User-facing contract (the entire change reduces to this):

```rust
// COMMON — no name in source, ever
let exec = Executor::open(&cfg)?;
let node = exec.create_node("my_node")?;

// BRIDGE — names explicit because they wire load-bearing semantics
let exec = Executor::open_multi(&[
    SessionSpec::new("zenoh", &cfg_z),
    SessionSpec::new("dds",   &cfg_d),
])?;
let field   = exec.create_node_on("field",   "zenoh")?;
let control = exec.create_node_on("control", "dds")?;
Bridge::pubsub_raw(&field, "/sensor/raw",
                   &control, "/sensor/raw", type_hash, qos)?;
```

```cpp
// common
nros::init(locator, domain_id);
nros::create_node(node, "my_node");

// bridge
nros::SessionSpec specs[] = {{"zenoh", &cfg_z}, {"dds", &cfg_d}};
nros::init_multi(specs, 2);
nros::create_node_on(field,   "field",   "zenoh");
nros::create_node_on(control, "control", "dds");
nros::bridge::pubsub_raw(field, "/x", control, "/x", type_hash, qos);
```

## Work Items

### 128.A — Linker-section registry runtime

Add the section walker to `nros-rmw-cffi`. Backends emit entries; the
runtime walks them on first `Executor::open` call. No dynamic loading;
section symbols are resolved at static link time.

- [ ] `128.A.1`: define section `.nros_rmw_init` (POSIX/ELF) +
  `__DATA,__nros_rmw_init` (Mach-O) in `nros-rmw-cffi`. Provide
  `__start_nros_rmw_init` / `__stop_nros_rmw_init` access via a small
  linker-script-aware helper (works without custom linker script on
  ELF; needs explicit symbol pair on Mach-O via `__attribute__((used,
  section("__DATA,__nros_rmw_init")))` and `getsectbynamefromheader`
  lookup, or a `__llvm.linker.section.start_*` symbol).
  **Files:** `packages/core/nros-rmw-cffi/src/lib.rs`,
  `packages/core/nros-rmw-cffi/src/section.rs` (new),
  `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`.
- [ ] `128.A.2`: implement `nros_rmw_cffi_walk_init_section()` that
  iterates entries and calls each as `extern "C" fn()`. Each fn calls
  `nros_rmw_cffi_register_named(<canonical-name>, &VTABLE)`. Walker is
  idempotent (atomic init-once flag).
  **Files:** `packages/core/nros-rmw-cffi/src/section.rs`,
  `packages/core/nros-rmw-cffi/src/lib.rs` (init-once flag).
- [ ] `128.A.3`: `Executor::open` resolution policy:
  - `NROS_RMW` env (POSIX) / `NROS_RMW` getenv-equivalent (RTOS) →
    look up registry.
  - exactly one registered → that one (no env needed).
  - more than one + no env → return error
    `NROS_RMW_RET_AMBIGUOUS_BACKEND` with the registered names listed
    in the error payload (caller can `eprintln` them).
  - zero registered → `NROS_RMW_RET_NO_BACKEND` with a hint.
  **Files:** `packages/core/nros-node/src/executor/open.rs`,
  `packages/core/nros-rmw-cffi/src/lib.rs` (resolution helper).
- [ ] `128.A.4`: bare-metal targets that strip unreferenced section
  symbols (Cortex-M with `gc-sections`) need `KEEP(.nros_rmw_init)` in
  their linker script. Ship a snippet in
  `packages/core/nros-rmw-cffi/cmake/nros-rmw-section.ld` and document
  in `nros-baremetal-common`'s README.

### 128.B — Backend section entries

Each backend ships a single linker-section entry that registers it
under its canonical name. Names are documented as public contract:
`"zenoh"`, `"dds"`, `"xrce"`, `"cyclonedds"`, `"uorb"`.

- [ ] `128.B.1`: `nros-rmw-zenoh` — replace the POSIX-only
  `.init_array` ctor with a section entry that fires on all targets.
  **Files:** `packages/zpico/nros-rmw-zenoh/src/lib.rs`,
  `packages/zpico/nros-rmw-zenoh-staticlib/src/lib.rs`.
- [ ] `128.B.2`: `nros-rmw-dds` (Rust crate) — same pattern.
  **Files:** `packages/dds/nros-rmw-dds/src/lib.rs`,
  `packages/dds/nros-rmw-dds-staticlib/src/lib.rs`.
- [ ] `128.B.3`: `nros-rmw-xrce-cffi` — same pattern; works on
  bare-metal too (registration must not depend on POSIX features).
  **Files:** `packages/xrce/nros-rmw-xrce-cffi/src/lib.rs`.
- [ ] `128.B.4`: `nros-rmw-cyclonedds` (C++) — emit the section entry
  from C++ (`__attribute__((used, section(".nros_rmw_init")))`).
  **Files:** `packages/dds/nros-rmw-cyclonedds/src/register.cpp`
  (new).
- [ ] `128.B.5`: drop legacy unnamed `nros_rmw_cffi_register` shim
  (Phase 128.D depends on this). Keep `nros_rmw_cffi_register_named`
  as the only registration entry.
  **Files:** `packages/core/nros-rmw-cffi/src/lib.rs`,
  `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`.

### 128.C — Core RMW-blindness

Delete all RMW-specific glue from core init paths.

- [ ] `128.C.1`: remove `#ifdef NROS_RMW_<NAME>` chain from
  `nros::init` (C++ inline header).
  **Files:** `packages/core/nros-cpp/include/nros/node.hpp`.
- [ ] `128.C.2`: remove the matching chain from `nros-c`'s
  `nros_init` if present.
  **Files:** `packages/core/nros-c/src/*.c` (audit and trim).
- [ ] `128.C.3`: remove `rmw-{zenoh,xrce,dds,cyclonedds}-cffi` cargo
  features from `nros` and `nros-node`. They become inert because the
  backend dep itself emits the section entry.
  **Files:** `packages/core/nros/Cargo.toml`,
  `packages/core/nros-node/Cargo.toml`.
- [ ] `128.C.4`: collapse the CMake staticlib matrix. One canonical
  `libnros_c.a` + `libnros_cpp.a`; backends ship as separate static
  libs (`libnros_rmw_zenoh.a`, etc.) with `--whole-archive` link so
  the section entry survives stripping. Drop `NANO_ROS_RMW` CMake
  var (or make it the source of `target_link_libraries(... NanoRos::Rmw::<name>)`
  injection for back-compat one cycle, then delete).
  **Files:** `packages/core/nros-c/CMakeLists.txt`,
  `packages/core/nros-cpp/CMakeLists.txt`,
  `packages/core/nros-c/cmake/NanoRosLink.cmake`,
  `packages/core/nros-c/cmake/NanoRosConfig.cmake`.
- [ ] `128.C.5`: remove the explicit `nros_rmw_<x>::register()` call
  from every example `main.rs` and integration test that has one.
  **Files:** `examples/native/rust/{zenoh,dds,xrce}/*/src/main.rs`,
  `examples/qemu-arm-baremetal/rust/zenoh/*/src/main.rs`,
  `examples/qemu-arm-freertos/rust/zenoh/*/src/main.rs`,
  `examples/zephyr/rust/zenoh/*/src/main.rs`,
  `packages/testing/nros-tests/tests/*.rs` (audit).

### 128.D — Manifest-only platform selection

RMW backends stop carrying `platform-<rtos>` features; they consume
only the canonical `nros-platform-cffi` ABI. Platform selection lives
in the user's manifest via a direct `nros-platform-<name>` dep.

**Phase-128 scope reduction (2026-05-16).** The full elimination of
`platform-<rtos>` features from RMW backends turned out to be wider
than this phase can absorb: `zpico-sys/build.rs` and
`xrce-sys/build.rs` use those features to pick the vendor C source
files, the `ZENOH_<RTOS>` macro, and the per-platform CFG paths.
Removing them would require folding `zpico-platform-shim` and
`xrce-platform-shim` into `nros-platform-cffi` first — and those
shims do more than rename symbols (smoltcp clock bridge, custom
`_z_open_serial_*` per-board, orin-spe IVC helpers, etc.). Wholesale
fold-in is queued behind a dedicated phase-129.

What landed in phase 128.D:

- **Auto-derive `posix` from `target_os` in both build scripts.**
  `zpico-sys` and `xrce-sys` no longer require a hosted POSIX
  consumer to enable `platform-posix` / `posix` explicitly when
  `target_os ∈ {linux, macos, *bsd, android}` and no other platform
  feature is selected. RTOS targets (`target_os = "none"`) still need
  the explicit selector because multiple RTOSes share the same
  target triple.
- The duplicate-axis problem on POSIX hosts (Cargo dep on the
  backend + explicit `platform-posix` feature) collapses for the
  common case; embedded consumers stay the same.

Deferred to follow-up:

- [x] `128.D.0` (this phase): auto-derive `posix` on hosted
  `target_os` in `zpico-sys/build.rs` + `xrce-sys/build.rs`. Removes
  the duplicate-axis requirement for POSIX consumers.
  **Files:** `packages/zpico/zpico-sys/build.rs`,
  `packages/xrce/xrce-sys/build.rs`.
- [ ] `128.D.1` (deferred → phase 129): delete `platform-{posix,zephyr,
  bare-metal,freertos,nuttx,threadx,orin-spe}` features from
  `packages/zpico/nros-rmw-zenoh/Cargo.toml`. Blocked on 128.D.3
  (the build script's per-RTOS C source picking still rides on
  these features).
- [ ] `128.D.2` (deferred → phase 129): same for `nros-rmw-xrce-cffi`.
- [ ] `128.D.3` (deferred → phase 129): fold `zpico-platform-shim`
  symbols into `nros-platform-cffi` via a C aliasing TU. The shim
  also carries smoltcp-clock bridges, per-board serial openers, and
  orin-spe IVC helpers that need a home before the crate can be
  deleted.
- [ ] `128.D.4` (deferred → phase 129): same fold for
  `xrce-platform-shim`.

### 128.E — Transport selection becomes runtime

`link-tcp`, `link-udp-unicast`, `link-tls`, `link-custom`, and the
parallel zenoh-pico `Z_FEATURE_LINK_*` macros stop being build-time
gates. Vendor C sources for every transport are always compiled; the
locator string at session-open picks which one runs.

- [ ] `128.E.1`: delete `link-*` cargo features from
  `nros-rmw-zenoh` and `zpico-sys`. Always-on
  `Z_FEATURE_LINK_TCP=1`, `LINK_UDP=1`, `LINK_SERIAL=1`,
  `LINK_TLS` only when the build has a TLS provider available
  (`OPENSSL_DIR` or `MBEDTLS_DIR` — preserve that build-host gate).
  **Files:** `packages/zpico/nros-rmw-zenoh/Cargo.toml`,
  `packages/zpico/zpico-sys/Cargo.toml`,
  `packages/zpico/zpico-sys/build.rs`.
- [ ] `128.E.2`: same audit for XRCE transports
  (`UCLIENT_PROFILE_UDP`, `TCP`, `SERIAL`).
  **Files:** `packages/xrce/xrce-sys/build.rs`.
- [ ] `128.E.3`: examples drop `link-*` from their `Cargo.toml`
  dependency feature list.
  **Files:** every example `Cargo.toml` under `examples/`.

### 128.F — Bridge crate (multi-RMW, names explicit)

Bridge support lives in a separate crate so the common-case umbrella
surface stays unchanged.

- [ ] `128.F.1`: `Executor::open_multi(&[SessionSpec])` keyed by name;
  resolves each name against the named registry. Reuses existing
  `extra_sessions` plumbing (already drives `drive_io(0)` per spin).
  **Files:** `packages/core/nros-node/src/executor/open.rs`,
  `packages/core/nros-node/src/executor/spin.rs`.
- [ ] `128.F.2`: `Executor::create_node_on(name, rmw)` binds a node
  to a specific session. `create_node(name)` keeps single-session
  semantics for the common case.
  **Files:** `packages/core/nros-node/src/executor/types.rs`,
  `packages/core/nros-node/src/node.rs`.
- [ ] `128.F.3`: new crate `packages/bridge/nros-bridge/` with
  `Bridge::pubsub_raw(src_node, src_topic, dst_node, dst_topic,
  type_hash, qos)`. Implementation: `RawSubscription` on the source
  side, `EmbeddedRawPublisher::publish_raw_with_attachment` on the
  destination side. Bytes pass through untouched (ROS-CDR on both
  sides assumed; type translation is out of scope).
  **Files:** `packages/bridge/nros-bridge/Cargo.toml` (new),
  `packages/bridge/nros-bridge/src/lib.rs` (new),
  `packages/bridge/nros-bridge/src/pubsub_raw.rs` (new).
- [ ] `128.F.4`: bidirectional bridge loop protection — tag forwarded
  messages via an attachment field (`bridge_origin = "<rmw>"`); drop
  on receive when origin matches local backend.
  **Files:** `packages/bridge/nros-bridge/src/pubsub_raw.rs`.
- [ ] `128.F.5`: C/C++ shim — `nros::init_multi`,
  `nros::create_node_on`, `nros::bridge::pubsub_raw` mirror the Rust
  surface 1:1.
  **Files:** `packages/core/nros-cpp/include/nros/bridge.hpp` (new),
  `packages/core/nros-c/include/nros/bridge.h` (new).

### 128.G — Optional config-driven entrypoint

For users who want to swap RMW without recompiling (still subject to
which backends were linked), add a TOML/JSON loader behind a feature
flag.

- [ ] `128.G.1`: `nros-bridge` gains `config = ["dep:toml"]` feature
  exposing `pub fn run_from_config(path: impl AsRef<Path>)`. Schema
  documented in `book/src/reference/nros-toml.md`.
  **Files:** `packages/bridge/nros-bridge/src/config.rs` (new),
  `book/src/reference/nros-toml.md` (new).
- [ ] `128.G.2`: end-user crate `nros` re-exports `run_from_config`
  behind `feature = "config"` that pulls `nros-bridge/config`.
  **Files:** `packages/core/nros/Cargo.toml`,
  `packages/core/nros/src/lib.rs`.

## Acceptance Criteria

A. **No backend name appears in core source.**
   - `git grep -nE 'NROS_RMW_(ZENOH|DDS|XRCE|CYCLONEDDS|UORB)'
     packages/core/nros packages/core/nros-c packages/core/nros-cpp
     packages/core/nros-node` returns 0 lines.
   - `git grep -nE 'nros_rmw_(zenoh|dds|xrce|cyclonedds|uorb)_register'
     packages/core/nros-c packages/core/nros-cpp packages/core/nros-node`
     returns 0 lines.

B. **No `register()` call in user examples for the common case.**
   - `git grep -n 'nros_rmw_.*::register' examples/` returns 0 lines.
   - **Stable Rust caveat:** rlib units that are not symbol-referenced
     from user code are NOT pulled into the final binary, even when
     they contribute to `RMW_INIT_ENTRIES`. The one-line
     `nros_rmw_<x>::register()` call doubles as the rlib-pull anchor
     AND a (redundant, idempotent) registration. C/C++ binaries do
     not need it because static-lib backends link with
     `--whole-archive` semantics. The call is preserved in Rust
     examples for that reason and documented as such. Acceptance B
     becomes "no `#ifdef`-style fan-out in core; one-line anchor
     allowed at the binary's entry point".

C. **Single-backend example builds + runs without naming the
   backend.**
   - `examples/native/rust/zenoh/talker` keeps no `register()` call;
     `cargo run` succeeds.
   - `examples/native/cpp/zenoh/talker` builds without
     `NANO_ROS_RMW`; `nros::init` succeeds.
   - bare-metal MPS2-AN385 + freertos QEMU + nuttx QEMU + zephyr
     native_sim equivalents all pass with no source-level register
     call.

D. **Multi-backend in one binary works via `Executor::open_multi`.**
   - New E2E test `multi_rmw_bridge_e2e` (see below) passes.

E. **`NROS_RMW=<name>` env var resolves ambiguity at runtime.**
   - Binary linked against both `nros-rmw-zenoh` and `nros-rmw-dds`
     refuses `Executor::open` with `RET_AMBIGUOUS_BACKEND` when env
     is unset, succeeds with either backend when env is set.

F. **`link-*` cargo features deleted; vendor builds all
   transports.**
   - `cargo metadata` shows no `link-tcp`, `link-udp-unicast`,
     `link-tls`, `link-custom` features on `nros-rmw-zenoh`.
   - Same QEMU bare-metal example, no rebuild, can switch locator
     between `tcp/...` and `serial/...` at runtime config (proves
     both transports are linked).

G. **Section walker is the only registration path.**
   - `git grep -nE 'nros_rmw_cffi_register\b'` returns 0 lines
     (only `nros_rmw_cffi_register_named` survives).
   - `nros_rmw_cffi_walk_init_section` runs at most once per process
     (verified by integration test that calls `Executor::open` twice
     and observes one registration log).

H. **Zero regression.**
   - `just ci` net-zero new failures vs `main` after step ordering
     completes.
   - All previously passing per-platform tests still pass:
     `just qemu test`, `just freertos test`, `just nuttx test`,
     `just zephyr test`, `just threadx_linux test`.

## E2E Tests

### 128.E2E.1 — common-case nameless boot, every platform/language

Verifies: acceptance C.

| Platform           | Language | Binary path                                                      | Expected output                  |
|--------------------|----------|------------------------------------------------------------------|----------------------------------|
| native POSIX       | Rust     | `examples/native/rust/zenoh/talker`                              | publishes; no `register()` call  |
| native POSIX       | C        | `examples/native/c/zenoh/talker`                                 | publishes; no `NANO_ROS_RMW=...` |
| native POSIX       | C++      | `examples/native/cpp/zenoh/talker`                               | publishes; no `NANO_ROS_RMW=...` |
| QEMU MPS2-AN385    | Rust     | `examples/qemu-arm-baremetal/rust/zenoh/talker`                  | publishes via TCP ethernet       |
| QEMU MPS2-AN385    | Rust     | `examples/qemu-arm-baremetal/rust/zenoh/serial-talker`           | publishes via UART serial        |
| QEMU FreeRTOS      | Rust     | `examples/qemu-arm-freertos/rust/zenoh/talker`                   | publishes                        |
| QEMU NuttX         | Rust     | `examples/qemu-arm-nuttx/rust/zenoh/talker`                      | publishes                        |
| Zephyr native_sim  | Rust     | `examples/zephyr/rust/zenoh/talker`                              | publishes                        |
| Zephyr native_sim  | C++      | `examples/zephyr/cpp/zenoh/talker`                               | publishes                        |
| ThreadX Linux      | Rust     | `examples/threadx-linux/rust/zenoh/talker`                       | publishes                        |

Harness:
```bash
just test-all
```
filtered to `e2e` group; explicit check that diff vs main shows zero
re-additions of `register(` / `NROS_RMW_<X>` per acceptance A/B.

### 128.E2E.2 — ambiguity guardrail

Verifies: acceptance E, G.

New test `packages/testing/nros-tests/tests/rmw_selection_ambiguity.rs`:

1. Build `talker_dual` fixture that depends on **both**
   `nros-rmw-zenoh` and `nros-rmw-dds`. No `register()` calls; no
   `NROS_RMW` env.
2. Run; assert `Executor::open` returns `RET_AMBIGUOUS_BACKEND` and
   the error payload lists both names.
3. Re-run with `NROS_RMW=zenoh`; assert success + topic flows via
   zenoh (router must be running on default port; verify via the
   existing `ZenohRouter` fixture).
4. Re-run with `NROS_RMW=dds`; assert success + topic flows via
   dust-DDS (verify via existing `DdsTalkerFixture`).
5. Re-run with `NROS_RMW=bogus`; assert
   `RET_UNKNOWN_BACKEND` with the available names in the payload.

### 128.E2E.3 — multi-RMW bridge

Verifies: acceptance D, F.G.4 loop protection.

New test `packages/testing/nros-tests/tests/multi_rmw_bridge_e2e.rs`:

1. Same `talker_dual` link contents; user code uses
   `Executor::open_multi(&[SessionSpec::new("zenoh", &cfg_z),
   SessionSpec::new("dds", &cfg_d)])`.
2. Create `field = create_node_on("field", "zenoh")` and
   `control = create_node_on("control", "dds")`.
3. Bridge `/sensor/raw` (`std_msgs/Int32`) from `field` to `control`
   via `Bridge::pubsub_raw`.
4. External zenoh-talker (`cargo run -p talker`) publishes 100 frames
   to `/sensor/raw`; external `ros2 topic echo /sensor/raw` running
   on DDS side records 100 frames within 10s.
5. Reverse direction: external DDS-talker → bridge → zenoh echo.
6. Bidirectional bridge (both directions configured); assert each
   message is delivered exactly once on the opposite side and never
   loops back (loop protection: messages tagged with
   `bridge_origin = <rmw>` are dropped when origin matches local
   backend).
7. Assert no `register()` call in the test source.

### 128.E2E.4 — bare-metal section walker

Verifies: acceptance G + 128.A.4 (linker-script `KEEP`).

Promote `examples/qemu-arm-baremetal/rust/zenoh/listener` to drop
its explicit `register()`. The MPS2-AN385 board crate's
`nros-rmw-section.ld` snippet is included via the existing
`memory.x` aggregation. QEMU run completes the listener's first
`Executor::open` and prints `Subscriber declared`. Captured QEMU
output is asserted by
`packages/testing/nros-tests/tests/emulator.rs::test_qemu_zenoh_talker_listener_e2e`
(no API change to that test; the binary contents under test change).

### 128.E2E.5 — config-driven run

Verifies: 128.G.

New test `packages/testing/nros-tests/tests/run_from_config.rs`:

1. Write a temp `nros.toml` with one `[[node]]` (zenoh) and one
   `[[bridge]]`.
2. Spawn binary built from
   `examples/native/rust/config-runner/` (new) whose `main` is
   literally `nros::run_from_config(std::env::args().nth(1).unwrap())`.
3. Source code under `examples/native/rust/config-runner/src/main.rs`
   must contain zero RMW names — assert via `grep -nE
   'zenoh|dds|xrce|cyclonedds'` returning 0 lines from the source.
4. Bridge end-to-end frame delivery verified the same way as
   128.E2E.3.

## Notes

- **No dynamic loading.** Phase 128 is purely link-time / compile-time.
  No `dlopen`, no plugin system, no shared libraries. Linker sections
  ARE the discovery mechanism, and they are populated at static link
  time. This matches the embedded targets' constraints (no loader,
  no filesystem) and removes a class of failure modes (mismatched
  ABI versions between plugin and host).
- **Names as public contract.** Once `"zenoh"`, `"dds"`, `"xrce"`,
  `"cyclonedds"`, `"uorb"` are documented as the canonical
  registration names, they cannot be renamed without a SemVer break.
  Document in `book/src/reference/rmw-backends.md`.
- **Cortex-M / RISC-V linker scripts.** Bare-metal targets must add
  `KEEP(.nros_rmw_init)` and matching `__start_/__stop_` symbols.
  The `nros-baremetal-common` crate ships the linker-script fragment
  so board crates only need `INCLUDE nros-rmw-section.ld;`.
- **Migration ordering matters.** Steps that delete code (128.C.5,
  128.D.3, 128.E.1) must land AFTER step 128.A (section walker) and
  step 128.B (backend entries) so existing binaries keep working at
  every commit. Each Work Item is independently revertable.
- **Bridge crate path.** Bridge support belongs in `packages/bridge/`
  to keep the umbrella crate (`nros`) free of multi-RMW concerns.
  The umbrella re-exports the bridge API behind a feature flag for
  users who do want it from one crate.
- **`xrce-zephyr` crate.** Likely deletable after 128.D.4 collapses
  the platform shim. Audit at the end of phase 128 and remove if
  unreferenced.
