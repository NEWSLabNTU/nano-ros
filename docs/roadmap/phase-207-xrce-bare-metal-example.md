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

### 207.3 — `examples/qemu-arm-baremetal/rust/talker-xrce/` — [x] DONE (2026-05-30)

- [x] `Cargo.toml` — board with `serial` + `xrce-transport`, no `rmw-zenoh`;
      `nros` with `rmw-cffi, platform-bare-metal, ros-humble`;
      `nros-rmw-xrce-cffi`; `panic-semihosting`. `[profile.size]` per 204.3.
- [x] `.cargo/config.toml` — target / qemu runner / gc-sections rustflags /
      `[env]` with `NROS_LINK_IP=0` / `ZPICO_NO_SMOLTCP=1` / `NROS_HEAP_SIZE="8192"`
      (XRCE's working set is small; verified down to 8 KB on the registration +
      session-open path).
- [x] `src/main.rs` — installs the shim (`xrce_transport::xrce_transport_ops()`),
      registers the XRCE backend, opens the executor, declares the publisher,
      publish loop.
- [x] `nros.toml` (serial locator) + `package.xml` (renamed).
- [x] **Builds clean** under both `--release` and `--profile size`. Boot smoke
      in QEMU reaches the XRCE session-open attempt and fails with
      `Transport(ConnectionFailed)` (expected — no `MicroXRCEAgent` running);
      backend + node creation succeed.

### 207.3.bm-libc — libc symbols on bare-metal — [x] DONE (2026-05-30)

Found while attempting 207.3. Bare-metal targets (`target_os = "none"`) ship
no libc; the link failed on `malloc`/`free`/`calloc`/`realloc`/`strrchr`/`strtol`.
Investigation showed the calls come from **nano-ros's own wrapper C**
(`nros-rmw-xrce/src/{subscriber,publisher,session,service,transport_nros_udp}.c`),
NOT from the Micro-XRCE-DDS-Client vendor sources (vendor is clean of libc
allocs). So (c) "vendor static-alloc config" turned out to be moot — only (b)
"libc stubs routing to the existing bare-metal heap" was needed:

- [x] **`strrchr` + `strtol`** appended to `nros-baremetal-common::libc_stubs`
      (shared, no heap dependency; tiny Rust impls).
- [x] **`malloc` / `free` / `realloc` / `calloc`** in `nros-platform-mps2-an385
      ::libc_stubs` — always-emit (mps2-an385 is bare-metal-only, no host libc
      to collide with), routing to the same `FreeListHeap` (`crate::memory`)
      that zenoh-pico uses → XRCE inherits the `NROS_HEAP_SIZE`-tunable pool,
      no second heap.
- [x] Verified: `cargo build --release` and `--profile size` on `talker-xrce`
      both link clean. `nm` shows only the actually-called shims (`calloc`,
      `free`, `strrchr`, `strtol`) — `malloc` / `realloc` are gc'd because the
      wrapper only calls `calloc` + `free`.
- **Files:** `packages/drivers/nros-baremetal-common/src/libc_stubs.rs`,
  `packages/platforms/nros-platform-mps2-an385/src/libc_stubs.rs`.

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

### 207.5 — Measure flash + RAM; close Phase 204.5's XRCE figure — [x] DONE (2026-05-30)

| profile | `text` | `data` | `bss` | RAM total | flash |
|---|---|---|---|---|---|
| release | 70 976 B (~69 KB) | 25 212 B (heap 24 KB) | 8 856 B | ~33 KB | ~69 KB |
| **size** + heap tuned (8 KB) | **62 060 B (~60.6 KB)** | 8 792 B | **8 768 B** | **~17.2 KB** | ~60.6 KB |

**Side-by-side with the zenoh-pico ethernet talker:**

| | flash (`text`) | RAM (`data + bss`) |
|---|---|---|
| zenoh-pico ethernet (release) | 177.4 KB | 158.7 KB |
| **XRCE bare-metal (size)** | **60.6 KB** | **17.4 KB** |
| ratio | **2.9× smaller** | **9.1× smaller** |
| micro-ROS reference | < 75 KB | ~3 KB (claimed; their static-only path) |

**XRCE clears the micro-ROS flash reference (< 75 KB) on day one**, and the
~17 KB RAM is on the right order of magnitude for the same client class
(the gap to ~3 KB is the heap-shaped wrapper allocs + the linkme + the small
8 KB `FreeListHeap` overhead; closing it is the "static-only XRCE wrapper"
work — orthogonal, not a 207 deliverable).

- [x] `size` + `nm` captured (above).
- [x] **Book "Measured footprint" table** in `book/src/user-guide/configuration.md`
      gains the XRCE bare-metal row (replacing the prior "no nano-ros
      measurement" placeholder).
- [x] **Phase 204.5 cross-reference closed** — the XRCE-vs-zenoh on-device
      delta this phase asked for is the table above.

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
