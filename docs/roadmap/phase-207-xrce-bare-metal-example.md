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

### 207.1 — `install_custom_transport` Rust surface in `nros-rmw-xrce-cffi`

A safe Rust API on top of the existing C `uxr_init_custom_transport` /
`uxrCustomTransport` struct. Takes the four callbacks (matching the XRCE
signatures) + a `*mut c_void` user context; stores them in a static (so the
addresses outlive the call) + invokes the C init. Must be called BEFORE the
backend's `register()` opens its first session.

- [ ] Add `pub fn install_custom_transport(...)` to `nros-rmw-xrce-cffi`
      (cfg-gated on `target_os = "none"` + any RTOS that needs custom).
- [ ] Build verified on `thumbv7m-none-eabi` + the existing hosted targets
      (the API compiles, gated to bare-metal callers; hosted continues to use
      the profile transports).
- [ ] Document the call ordering (install BEFORE `register()`).
- **Files:** `packages/xrce/nros-rmw-xrce-cffi/src/lib.rs`,
  `packages/xrce/nros-rmw-xrce-cffi/build.rs` (no new vendor sources — the
  `custom_transport.c` already compiles per 204.7).

### 207.2 — UART custom-transport shim on `nros-platform-mps2-an385`

A Rust module exposing four `extern "C" fn` callbacks that drive the CMSDK
`UART0` that `zpico-serial` already wraps. The shim is feature-gated so it
only compiles when the example asks for XRCE.

- [ ] `pub mod xrce_uart_transport` in `nros-platform-mps2-an385`, behind a
      `xrce-transport` Cargo feature.
- [ ] Reuse the `UART_DEVICE` static + the existing register-init from
      `node::install_uart()`; expose `open/close/write/read` callbacks with the
      XRCE signature (byte buffers + timeout); read is non-blocking with a
      poll budget.
- [ ] Smoke: shim compiles cleanly + the `MPS2_UART_XRCE` constant
      (`uxrCustomTransport` parameter set) is reachable from an example.
- **Files:** `packages/platforms/nros-platform-mps2-an385/src/xrce_uart_transport.rs`,
  `packages/platforms/nros-platform-mps2-an385/src/lib.rs` (re-export),
  `packages/platforms/nros-platform-mps2-an385/Cargo.toml` (feature).

### 207.3 — `examples/qemu-arm-baremetal/rust/talker-xrce/`

The shipped end-to-end example, parallel to `serial-talker` (zenoh) — same
`mps2-an385` board, same `[profile.size]` + size knobs from 204, but with the
XRCE backend + UART custom transport.

- [ ] `Cargo.toml` — board with `serial` + `rmw-xrce` + `xrce-transport`; `nros`
      with `rmw-cffi, platform-bare-metal, ros-humble`; `nros-rmw-xrce-cffi`
      backend; `panic-semihosting`.
- [ ] `.cargo/config.toml` — target triple, runner (qemu PTY), `rustflags`
      with `--gc-sections`, `[env]` with `NROS_LINK_IP=0` /
      `ZPICO_NO_SMOLTCP=1` / `NROS_HEAP_SIZE="8192"` (XRCE's claimed working
      set is ~3 KB; size with margin).
- [ ] `src/main.rs` — install the shim, register the XRCE backend, open the
      executor, declare publisher, publish in a timer.
- [ ] `[profile.size]` (Phase 204.3) + `nros.toml` (serial transport,
      `xrce/UART_0` locator).
- **Files:** `examples/qemu-arm-baremetal/rust/talker-xrce/{Cargo.toml,
  .cargo/config.toml, src/main.rs, nros.toml, package.xml}`.

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
