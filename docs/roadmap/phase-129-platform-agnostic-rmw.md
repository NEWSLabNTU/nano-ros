# Phase 129 — Platform-Agnostic RMW Backends

Date: 2026-05-17
Goal: RMW packages depend ONLY on the canonical `nros_platform_*`
  ABI. No `platform-<rtos>` cargo features. No per-RTOS vendor C
  source selection in the RMW build script. A single
  `nros-rmw-<name>` rlib + a single set of vendor C objects link
  against whichever platform provider the consumer wired in.
Status: planning
Priority: medium (logical follow-up to phase 128's manifest-driven
  selection; user expectation that RMW backends "do not have to know
  the platform" once they consume the platform ABI)
Depends on: phase 128 (manifest-driven RMW selection),
  phase 121 (platform C ABI canonical),
  phase 123.A.1.x.2 (POSIX C-port canonicalization)

## Overview

After phase 128 every consumer can pick its RMW backend at link time
via `nros-rmw-<name>` Cargo deps + `NanoRos::Rmw::<name>` cmake
targets. But the backends themselves still carry per-platform Cargo
features (`platform-posix`, `platform-zephyr`, `platform-bare-metal`,
…). Those features key vendor C source selection inside
`zpico-sys/build.rs` / `xrce-sys/build.rs`: a freertos build pulls
zenoh-pico's `src/system/freertos/system.c`, a nuttx build pulls
the unix `system.c` with `ZENOH_NUTTX`, etc.

The user's stated expectation: a backend that consumes the
`nros_platform_*` ABI should NOT need to know the platform name.
It links against the ABI; the platform provider (`nros-platform-cffi`
+ a board crate / `nros-platform-{posix,zephyr,freertos,…}` C port)
satisfies the symbols.

This phase finishes the work phase 128.D / 128.E.1 started:

1. **Generic platform adapter** replaces zenoh-pico's per-RTOS
   `system/<rtos>/system.c` selection. Memory / sleep / random /
   time / threading / mutex / condvar / task — every zenoh-pico
   platform symbol routes through `nros_platform_*` instead of
   per-RTOS code in the vendor tree.
2. **Network transport** stays a separate, narrowly-scoped axis.
   Zenoh-pico's `_z_sys_net_*` socket primitives are wired by
   `nros-smoltcp` (bare-metal), `nros-platform-posix` (hosted),
   `nros-platform-zephyr` (Zephyr sockets), etc. Backend doesn't
   pick — the consumer's `nros-platform-<rtos>` dep supplies the
   provider.
3. **Result**: `nros-rmw-zenoh` has ONE Cargo feature axis:
   `rmw-cffi` / `std` / `alloc` / `link-tls` (TLS-provider
   opt-in). No `platform-<rtos>`. No `link-tcp`/`link-udp` (already
   always-on after phase 128.E.0). Same for `nros-rmw-xrce-cffi`.

## Architecture

```
                       ┌──────────────────────────┐
                       │   user manifest          │
                       │                          │
                       │   nros-rmw-zenoh         │ ← RMW backend dep
                       │   nros-platform-<rtos>   │ ← platform provider dep
                       └────────────┬─────────────┘
                                    │
                          link-time resolution
                                    │
       ┌────────────────────────────▼────────────────────────────┐
       │  zenoh-pico vendor C (one set of TUs)                   │
       │                                                         │
       │  z_malloc / z_free / z_sleep_* / z_random_*             │
       │  _z_task_*  / _z_mutex_*  / _z_condvar_*                │
       │  _z_sys_net_socket_* / _z_sys_net_endpoint_*            │
       │                                                         │
       │  every symbol declared in zenoh-pico's vendor `system/  │
       │  common/platform.h` is satisfied by the generic adapter │
       │  emitted by zpico-sys (no per-RTOS conditional inside)  │
       └────────────────────────────┬────────────────────────────┘
                                    │ extern "C" call
                                    ▼
       ┌─────────────────────────────────────────────────────────┐
       │  nros_platform_* canonical ABI (nros-platform-cffi /    │
       │  nros-platform-api)                                     │
       └────────────────────────────┬────────────────────────────┘
                                    │ implemented by
                                    ▼
       ┌─────────────────────────────────────────────────────────┐
       │  nros-platform-{posix,zephyr,freertos,nuttx,threadx,…}  │
       │  (Rust crate OR C-port lib — orthogonal to backend)     │
       └─────────────────────────────────────────────────────────┘
```

Same shape for XRCE: `nros-rmw-xrce-cffi` compiles the
`micro-XRCE-DDS-Client` vendor sources against `nros_platform_*`
instead of picking `transport_posix_*.c` per platform.

## Work Items

### 129.A — Generic platform adapter (zenoh)

- [ ] `129.A.1` — extend `platform_aliases.c` (added in phase
  128.D.3) to cover the full `<system/common/platform.h>` symbol
  set. Today it covers memory / sleep / random / time. Add:
  threading (`_z_task_init/join/detach/cancel/exit`,
  `_z_task_free`), mutexes (`_z_mutex_init/drop/lock/try_lock/unlock`),
  recursive mutexes (`_z_mutex_rec_*`), condvars
  (`_z_condvar_init/drop/signal/signal_all/wait/wait_until`), and
  yields (`_z_yield`).
  **Files:** `packages/zpico/zpico-sys/c/zpico/platform_aliases.c`.
- [ ] `129.A.2` — extend `nros_platform_*` ABI where missing
  symbols are needed. Phase 121 already shipped task / mutex /
  condvar primitives; verify zenoh-pico's expectations match
  one-to-one. Patch gaps in
  `packages/core/nros-platform-cffi/include/nros/platform.h`.
- [ ] `129.A.3` — exclude zenoh-pico's per-RTOS `system/<rtos>/
  system.c` from the cc build when `platform-aliases` is on.
  Today `zpico-sys/build.rs` picks the file based on
  `CARGO_FEATURE_<RTOS>`. New mode: opt out of vendor `system.c`
  entirely, link only the alias TU.
  **Files:** `packages/zpico/zpico-sys/build.rs`.
- [ ] `129.A.4` — make `platform-aliases` the default. Once A.1–A.3
  prove out on POSIX + a sample RTOS, flip the default. Existing
  `platform-<rtos>` features become inert markers (deleted in
  129.C).

### 129.B — Generic platform adapter (XRCE)

- [ ] `129.B.1` — same fold for `nros-rmw-xrce-cffi`. Today
  `build.rs` compiles `transport_posix_udp.c` / `transport_posix_serial.c`
  / `transport_zephyr_udp.c` conditionally. The new mode wires the
  micro-XRCE custom-transport callbacks through `nros_platform_*`
  net symbols and drops the per-platform transport TUs entirely.
- [ ] `129.B.2` — gap-fill on the `nros_platform_*` net surface.
  Phase 121 covered TCP/UDP socket primitives; verify XRCE's
  callback signatures map 1:1.

### 129.C — Delete `platform-<rtos>` features from RMW crates

- [ ] `129.C.1` — `nros-rmw-zenoh/Cargo.toml`: remove
  `platform-{posix,zephyr,bare-metal,freertos,nuttx,threadx,orin-spe}`
  features. Any forwarding cleanup in `zpico-sys`.
- [ ] `129.C.2` — `nros-rmw-xrce-cffi/Cargo.toml`: same.
- [ ] `129.C.3` — board crates that flip these features in their
  RMW deps drop them. Verify with `git grep -nE
  'platform-(posix|zephyr|bare-metal|freertos|nuttx|threadx|orin-spe)'
  packages/boards/ examples/` after the deletion.

### 129.D — Delete `zpico-platform-shim` + `xrce-platform-shim`

- [ ] `129.D.1` — `zpico-platform-shim` rehomes its remaining
  responsibilities (smoltcp clock bridge, per-board serial
  openers, orin-spe IVC helpers) into their respective board
  crates / driver crates. Each board / driver exports the symbol
  it needs directly via `#[no_mangle]`. Crate deletion follows.
- [ ] `129.D.2` — same for `xrce-platform-shim`.

### 129.E — Examples + fixtures sweep

- [ ] `129.E.1` — drop the now-inert `platform-<rtos>` feature
  names from every example `Cargo.toml`. Same shape as phase
  128.H.2 (manifest fixups) — surface a grep + edit list before
  touching files.
- [ ] `129.E.2` — `cargo build --workspace` + per-platform
  `just <plat> build-all` sweep, same as phase 128.H.6.

## Acceptance Criteria

A. **RMW crates declare zero platform features.**
   - `grep -E '^platform-' packages/zpico/nros-rmw-zenoh/Cargo.toml
     packages/xrce/nros-rmw-xrce-cffi/Cargo.toml` returns 0 lines.

B. **Vendor C build is platform-blind.**
   - `zpico-sys/build.rs` no longer references
     `CARGO_FEATURE_<RTOS>` for source-file selection (except for
     the `link-tls` / `link-ivc` capability gates).
   - Same audit pass on `xrce-sys/build.rs`.

C. **Platform-shim crates deleted.**
   - `ls packages/zpico/zpico-platform-shim/
     packages/xrce/xrce-platform-shim/` returns "No such file or
     directory".
   - `git grep -n 'extern crate zpico_platform_shim'` returns 0.

D. **End-to-end works on every supported platform.** Same matrix
   as phase 128.H.6 — qemu, freertos, nuttx, threadx_linux, zephyr,
   esp32, cyclonedds — all `build-all` recipes green.

## Notes

- Threading is the hardest fold. zenoh-pico's `_z_task_t` /
  `_z_mutex_t` / `_z_condvar_t` are opaque structs whose
  on-stack/in-static storage size differs per platform (FreeRTOS
  pads for TCB embedding, ThreadX uses TX_THREAD layout, POSIX
  uses pthread_t alignment). The generic adapter needs a
  worst-case storage size (matching the largest of any supported
  platform) and a runtime check that the active platform's
  expected size fits. `nros-platform-cffi` already declares
  `NROS_PLATFORM_TASK_OPAQUE_U64S` etc. — verify zenoh-pico's
  pre-existing `_z_task_t` storage matches or extend.
- Network is the second-hardest fold. Zenoh-pico's
  `_z_open_*` / `_z_send_*` / `_z_read_*` per-link-type signatures
  are slightly different across TCP / UDP / serial. The generic
  adapter routes each through the appropriate `nros_platform_*`
  net call; per-board adapters (`nros-board-mps2-an385`,
  `nros-board-esp32`) keep their physical-layer wiring.
- TLS keeps its build-host capability gate (`link-tls` feature)
  because mbedTLS / OpenSSL are real link deps the platform ABI
  cannot abstract.
- ROS edition (`ros-humble` / `ros-iron`) stays per-RMW because
  the wire / type-hash conventions differ across editions and the
  backend implements them directly.
