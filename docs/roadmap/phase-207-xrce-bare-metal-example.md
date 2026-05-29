# Phase 207 — XRCE on bare-metal: custom-transport surface + example

**Goal.** Ship a working bare-metal `qemu-arm-baremetal/rust/talker-xrce` (and
later listener / serial-talker counterparts) that publishes to
`MicroXRCEAgent` over UART, so nano-ros has a measured XRCE-class footprint
cell at last (the open follow-on from Phase 204.5 → moved here). The blocker
is purely the **custom-transport injection** that XRCE requires on
`target_os = "none"`; everything upstream (the XRCE cffi backend, the
multi-RMW registration, `rmw-xrce` cargo feature, agent provisioning) already
works on hosted targets.

**Status.** Proposed (2026-05-30).

**Priority.** P2 — unblocks the honest XRCE-vs-zenoh RAM comparison Phase 204
deliberately left open, and is the on-ramp to the micro-ROS-class footprint
("XRCE + serial + static pools") that today's nano-ros can describe but not
ship as a bare-metal example.

**Depends on.** None new — the infrastructure landed in earlier phases:
- Phase 115.K.2 — `nros-rmw-xrce-cffi` C shim + multi-RMW registration.
- Phase 142 — bare-metal RMW registration anchor via the explicit
  `nros_rmw_<x>::register()` call (the `RMW_INIT_ENTRIES` stub on `target_os = "none"`).
- Phase 204.7 — `NROS_LINK_IP=0` (serial-only sheds the IP link C).
- Phase 204.5 — env-tunable static heap (`NROS_HEAP_SIZE`).
- Existing `nros-rmw-xrce-cffi/build.rs` already emits the bare-metal profile
  set: `UCLIENT_PROFILE_{CUSTOM_TRANSPORT, STREAM_FRAMING, DISCOVERY}` (everything
  bare-metal XRCE needs), with `UDP/TCP/SERIAL_POSIX` excluded via `is_posix`
  gating. The C side is ready; only the Rust install hook + the per-board
  transport shim + the example are missing.

## Overview

Micro-XRCE-DDS-Client on a hosted RTOS picks its transport from
`UCLIENT_PROFILE_UDP/TCP/SERIAL` (compiled into `transport_posix_*.c`,
`transport_freertos_*.c`, …). On bare-metal **none of those profiles compile**
(they reference libc / RTOS sockets / fcntl). XRCE supports exactly one
escape hatch in that mode: `UCLIENT_PROFILE_CUSTOM_TRANSPORT`, where the
application registers four function pointers — `open`/`close`/`write`/`read` —
and XRCE drives the rest of the stream framing + discovery + RTPS-over-XRCE
on top.

So the bring-up is three layers, all small:
1. **CFFI surface.** A Rust install hook in `nros-rmw-xrce-cffi` that takes
   four `extern "C" fn` pointers + a context, fills XRCE's
   `uxrCustomTransport` struct, and calls `uxr_init_custom_transport` before
   the session opens. Hosted XRCE bring-up uses the equivalent profile call
   internally — bare-metal exposes it to the user.
2. **Per-board shim.** On the mps2-an385 (cortex-m3 + CMSDK UART) the shim is
   a Rust module that wraps the same `UART0` `zpico-serial` already uses,
   adapted to the XRCE callback signatures (byte-oriented `write` / `read`
   with timeouts). Same pattern works for stm32f4 + qemu-esp32-baremetal.
3. **Example.** `examples/qemu-arm-baremetal/rust/talker-xrce/` — the usual
   `Cargo.toml` (board with `serial` + `rmw-xrce`) + `main.rs` that calls the
   install hook with the shim, then `nros_rmw_xrce::register()` (the
   bare-metal linkage anchor), `Executor::open`, `create_publisher`, publish.

Measured against `MicroXRCEAgent` over the host PTY (the same socat bridge
the `test_qemu_serial_pubsub_e2e` already drives for zenoh-pico), this lights
up an end-to-end XRCE bare-metal cell and unlocks the missing 204.5 footprint
number.

## Architecture

```
+-------------------------------------+
|  examples/qemu-arm-baremetal/       |
|    rust/talker-xrce/main.rs         |
|      └─ nros_rmw_xrce::             |
|         install_custom_transport(   |
|           &mps2_uart_xrce_shim)     |
|      └─ nros_rmw_xrce::register()   |
|      └─ Executor::open + publish    |
+-----------------|-------------------+
                  v
+--- nros-rmw-xrce-cffi (Rust → C) ---+
|  pub fn install_custom_transport(   |
|    open: extern "C" fn(...),        |
|    close, write, read,              |
|    ctx: *mut c_void)                |
|  → fills uxrCustomTransport         |
|  → uxr_init_custom_transport(...)   |
+-----------------|-------------------+
                  v
+--- micro-xrce-dds-client (vendor) --+
|  profile/transport/custom/          |
|  custom_transport.c  (already in    |
|  the bare-metal build per 204.7)    |
+-----------------|-------------------+
                  v
+--- board shim (mps2_uart_xrce) -----+
|  open()  → enable UART0             |
|  read()  → poll RX FIFO into buf    |
|  write() → push buf into TX FIFO    |
|  close() → disable UART0            |
+-----------------|-------------------+
                  v
            QEMU UART0 PTY  <-->  socat  <-->  MicroXRCEAgent (host)
```

## Work Items

### 207.1 — `install_custom_transport` Rust surface — [x] DONE (2026-05-30)

The C plumbing already existed in `nros-rmw-xrce/src/transport_custom.c`
(`nros_rmw_xrce_set_custom_transport_ops` + the four `uxrCustomTransport`
trampolines, Phase 115.K.2.4); the `nros-rmw-xrce-cffi` build.rs already
links it (line 141: `transport_custom`). So 207.1 collapsed to adding the
Rust binding + safe wrapper.

- [x] **`NrosRmwXrceTransportOps` `#[repr(C)]` struct + `extern "C"` decl for
      `nros_rmw_xrce_set_custom_transport_ops`** in `nros-rmw-xrce-cffi`.
      Layout-identical to the C `nros_rmw_xrce_transport_ops_t`; `open`/`close`/
      `write`/`read` are `Option<unsafe extern "C" fn(...)>` so callers can
      pass `None` for unused callbacks (C side null-checks).
- [x] **`pub unsafe fn set_custom_transport_ops(&ops, framing)`** — safe Rust
      wrapper returning `Result<(), RegisterError>`. Documented call ordering
      (install BEFORE `register()` / `Executor::open`).
- [x] Verified `cargo build -p nros-rmw-xrce-cffi` + `cargo test -p
      nros-rmw-xrce-cffi` green (linkme stub no-op test passes; the existing
      `register_smoke` continues to pass — surface is additive).
- **Files:** `packages/xrce/nros-rmw-xrce-cffi/src/lib.rs`.

### 207.2 — UART custom-transport shim on `nros-board-mps2-an385` — [x] DONE (2026-05-30)

(Landed on the **board** crate, not `nros-platform-*` — `UART_DEVICE` lives in
the board where the hardware init runs; the platform crate is one abstraction
layer up.)

- [x] **`xrce-transport` Cargo feature** on `nros-board-mps2-an385`, forwards
      through to `serial` + pulls `nros-rmw-xrce-cffi` (the type source).
- [x] **`pub mod xrce_transport`** with four `extern "C" fn`s
      (`xrce_open/close/write/read`) bound to the same `UART_DEVICE`
      (`cmsdk_uart::CmsdkUart`) the zenoh-pico serial path uses; `UART_DEVICE`
      made `pub(crate)` so the new module shares it without a re-init.
- [x] **`pub fn xrce_transport_ops()`** factory returning
      `NrosRmwXrceTransportOps` (avoids the `*mut c_void` → `!Sync` static
      issue; the small struct is constructed at call time).
- [x] Smoke: the board crate still builds clean with default features
      (verified via `serial-talker` rebuild), and the new module compiles when
      a downstream example enables `xrce-transport`.
- **Files:** `packages/boards/nros-board-mps2-an385/src/xrce_transport.rs`
  (new), `…/src/lib.rs` (re-export gated on feature),
  `…/src/node.rs` (`UART_DEVICE` → `pub(crate)`),
  `…/Cargo.toml` (feature + dep).

### 207.3 — `examples/qemu-arm-baremetal/rust/talker-xrce/` — [~] skeleton landed; **blocked on 207.3.bm-libc**

Files laid down + the surface wired through; the link step surfaces the next
real bring-up gap (see 207.3.bm-libc below).

- [x] **`Cargo.toml`** — board with `serial` + `xrce-transport`, no
      `rmw-zenoh`; `nros` with `rmw-cffi, platform-bare-metal, ros-humble`;
      `nros-rmw-xrce-cffi`; `panic-semihosting`. `[profile.size]` per 204.3.
- [x] **`.cargo/config.toml`** — copied from `serial-talker` (same target /
      runner / rustflags / `[env]` with `NROS_LINK_IP=0` / `ZPICO_NO_SMOLTCP=1`
      / `NROS_HEAP_SIZE="24576"`). XRCE's smaller working set means the heap
      can likely come down further (target ~8 KB once a passing build exists).
- [x] **`src/main.rs`** — installs the shim, registers the XRCE backend,
      opens the executor, declares publisher, publish loop. Uses the
      `nros_rmw_xrce_cffi::set_custom_transport_ops` API from 207.1 + the
      `xrce_transport::xrce_transport_ops()` factory from 207.2.
- [x] `nros.toml` (serial locator) + `package.xml` (renamed).
- [→] **Build link FAILS — `nros-rmw-xrce`'s C session/publisher/service
      reference libc** (`malloc`/`free`/`strrchr`/`strtol`) which don't exist
      on `target_os = "none"`. zenoh-pico runs bare-metal because `zpico-sys`
      provides picolibc / errno-override plumbing; the `nros-rmw-xrce-cffi`
      build.rs does not yet do the equivalent. Tracked as 207.3.bm-libc.
- **Files:** `examples/qemu-arm-baremetal/rust/talker-xrce/{Cargo.toml,
  .cargo/config.toml, src/main.rs, nros.toml, package.xml}`.

### 207.3.bm-libc — provide libc symbols to the XRCE bare-metal link

Discovered while attempting 207.3. The vendor Micro-XRCE-DDS-Client + the
nros-rmw-xrce wrapper call `malloc`/`free`/`strrchr`/`strtol` from
`session.c`/`publisher.c`/`service.c`. Bare-metal targets (`target_os =
"none"`) ship no libc. Three plausible paths:

1. **picolibc on the XRCE side**, mirroring `zpico-sys`'s
   `needs_picolibc`/`needs_errno_override` plumbing (`nros-rmw-xrce-cffi`
   build.rs gains the same arch-table → `-isystem $picolibc/include` +
   newlib stubs). Cleanest, matches the existing pattern.
2. **Route XRCE's allocations through `zpico-alloc`** (the bare-metal
   free-list heap already linked) by providing `malloc`/`free` wrappers in
   `nros-platform-mps2-an385` (or a new shared `nros-platform-libc-stubs`
   crate); `strrchr`/`strtol` stubs are tiny.
3. **Patch / config-flag the vendor** to a static-only allocation mode
   (`RMW_UXRCE_ALLOW_DYNAMIC_ALLOCATIONS=OFF`-style, micro-ROS does this) so
   it never calls `malloc`/`free` in the first place. The string helpers
   stay; same handful of stubs.

- [ ] Pick the path (the right answer is likely (2)+(3) — small libc-stub
      crate + static-allocation vendor config — both keep the build
      reproducible and avoid pulling picolibc into a flow that doesn't need
      its math lib).
- [ ] Land it so `cargo build --release` on `talker-xrce` links clean.
- **Files (likely):** `packages/xrce/nros-rmw-xrce-cffi/build.rs` (config
  defines), a new `nros-platform-*` shim or shared crate (libc stubs),
  potentially `packages/xrce/nros-rmw-xrce-cffi/src/include/nros_xrce_config.h`
  (vendor static-alloc defines).

### 207.4 — E2E test against `MicroXRCEAgent` over a socat PTY bridge

A new `nros-tests` integration analogous to `test_qemu_serial_pubsub_e2e`
(zenoh), but exercising the XRCE agent end of the bridge.

- [ ] `test_qemu_xrce_pubsub_e2e` in `packages/testing/nros-tests/tests/emulator.rs`
      — start `MicroXRCEAgent` on a socat PTY pair, launch the talker-xrce
      firmware on the matching QEMU PTY, assert at least one
      `Published`/`Received` line (or a ROS-side subscriber count).
- [ ] Fixture: prebuilt talker-xrce + (later) listener-xrce, mirroring the
      serial fixture pattern.
- [ ] Skip cleanly when ARM toolchain / qemu / `socat` / `MicroXRCEAgent`
      absent; pass under `nros-fast-release` like the serial pair.
- **Files:** `packages/testing/nros-tests/tests/emulator.rs`,
  `packages/testing/nros-tests/src/fixtures/binaries/mod.rs`.

### 207.5 — Measure flash + RAM; close Phase 204.5's XRCE figure

Once 207.3 + 207.4 are green, the missing XRCE-class footprint cell exists.

- [ ] `size` + `nm` on the release + `--profile size` `talker-xrce` ELF; capture
      `text` / `data` / `bss` + the static-pool / heap breakdown.
- [ ] Add the row to `book/src/user-guide/configuration.md` "Measured
      footprint" table (the empty XRCE bare-metal cell).
- [ ] Cross-reference Phase 204.5; the XRCE-vs-zenoh on-device delta is the
      concrete answer to the question that phase left open.

## Acceptance

- [ ] `qemu-arm-baremetal/rust/talker-xrce` builds, boots in QEMU, and
      exchanges at least one message with `MicroXRCEAgent` over the host PTY
      bridge (`test_qemu_xrce_pubsub_e2e` green under `nros-fast-release`).
- [ ] Measured flash + RAM in the book size table.
- [ ] `nros-rmw-xrce-cffi::install_custom_transport` is the documented entry
      point any other bare-metal board can reuse (the shim is per-board, the
      install hook is shared).

## Notes

- **micro-ROS reference.** Micro-XRCE-DDS-Client documents `< 75 KB flash /
  ~3 KB RAM` (pub+sub, 512 B msgs) with static-only allocations
  (`RMW_UXRCE_ALLOW_DYNAMIC_ALLOCATIONS=OFF` + the `RMW_UXRCE_MAX_*` pools).
  The first nano-ros bare-metal XRCE cell won't match that on day one — it'll
  use the same alloc path zenoh-pico does — but is the on-ramp to it.
- **XRCE on `target_os = "none"`.** The cffi build.rs intentionally compiles
  only the `CUSTOM_TRANSPORT + STREAM_FRAMING + DISCOVERY` profiles; UDP/TCP/
  SERIAL POSIX transports are excluded. This is the right shape — XRCE's
  bare-metal contract is "you provide the bytes, we frame + agent the rest" —
  it just needs the install hook + the board shim to come alive.
- **Why a board shim, not a board crate change.** The shim is a thin
  wrapper around the same `UART_DEVICE` `zpico-serial` already uses; keeping
  it behind a `xrce-transport` feature avoids regressing the existing zenoh
  serial path (no extra symbols pulled when XRCE isn't selected).
- **Future cells.** Once mps2-an385 lights up, the same pattern lifts to
  stm32f4 (USART) and qemu-esp32-baremetal (UART) — each gets its own
  `xrce_uart_transport` shim + `talker-xrce` example. The install hook is
  shared.
