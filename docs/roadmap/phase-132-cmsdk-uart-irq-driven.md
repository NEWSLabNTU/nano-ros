# Phase 132 — IRQ-driven CMSDK UART for MPS2-AN385 serial transport

**Goal:** make the QEMU MPS2-AN385 serial pub/sub E2E
(`test_qemu_serial_pubsub_e2e`) pass without breaking the zenoh init
handshake. Replace the synchronous busy-spin TX in
`packages/drivers/cmsdk-uart/src/lib.rs::CmsdkUart::write` with an
IRQ-driven design that yields cleanly to QEMU's main loop between
bytes.

**Status:** structural scaffolding landed (ISR symbol exported
from `cmsdk-uart`, board-crate `#[interrupt]` forward, opt-in PAC
dep, `zpico-sys` `ZPICO_NO_SMOLTCP` env opt-out for serial-only
embedded builds broken since Phase 128). The IRQ-driven `write`
implementation is **deferred** — `test_qemu_serial_pubsub_e2e`
hangs at zenoh-pico's session-init handshake regardless of
whether the IRQ path is active. Reproduced with the polled
busy-spin write and the IRQ wiring fully removed: BOTH talker
and listener stop at "Zenoh locator: serial/UART_0..." and never
emit the post-`Executor::open` "Declaring publisher" /
"Subscribing to /chatter" lines. The original 127.D.3 hang at
"Publishing messages over serial..." (FIFO-fill on the second
publish) is downstream of init — we never reach it now. The
actual bug is in the connect_serial Init/InitAck exchange
between zenoh-pico's bare-metal `_z_open_serial_from_dev` and
zenohd's z-serial plugin, ambient since Phase 128/129/134
reshaped the zpico-sys build path.

Phase 132's IRQ-driven write was the wrong fix for the surface
symptom. The new failure mode (init handshake) needs separate
investigation: check the socat PTY bridge, zenohd z-serial
plugin compatibility, and the COBS framing on both sides.
Tracking under a new phase will be cleaner than expanding 132's
scope; 132's structural pieces land here so the future IRQ wire-
in is one feature flag away.

Spun out from
[`phase-127-remaining-failure-groups.md`](phase-127-remaining-failure-groups.md)
`127.D.3` on 2026-05-17 after the D.1 / D.2 / D.4 portions of 127.D
closed.

**Priority:** medium. Serial pub/sub is the last failing entry in the
127.D bucket; service / action / pub-sub over slirp + LAN9118 are
all green under the patched QEMU.

**Depends on:** none. Patched QEMU
(`third-party/qemu/qemu` + `third-party/qemu/patches/`) is already in
place from 127.A/D, but the bug reproduces under stock QEMU as
well — this is a guest-side spin-loop issue, not a model-side
flow-control issue.

## Overview

Non-RTIC `run()` serial examples sit in a tight loop:

```rust
loop {
    for _ in 0..100u32 {
        executor.spin_once(core::time::Duration::from_millis(10));
    }
    publisher.publish(...)?;
}
```

The first `publish(...)` reaches
`CmsdkUart::write` (`packages/drivers/cmsdk-uart/src/lib.rs`):

```rust
for &byte in data {
    while self.tx_full() {
        core::hint::spin_loop();
    }
    self.write_reg(DATA, byte as u32);
}
```

Under QEMU `-icount shift=auto` (already required for our smoltcp
tests; see `docs/reference/qemu-icount.md`), `core::hint::spin_loop`
runs entirely inside the guest's CPU emulation. The CMSDK UART
model only ticks when QEMU's main loop runs, which only happens
when the guest yields (WFI / WFE / SVC / IRQ). With no yield path
in `CmsdkUart::write`, `tx_full` stays true forever after the
guest fills its 16-byte TX FIFO, and the write blocks indefinitely.

Session-open handshake bytes succeed because they happen before
the FIFO has had time to fill; the first `publish` after open
trips the FIFO-full branch and the guest hangs.

## Architecture

CMSDK APB UART has TX-empty and TX-interrupt-enable bits. QEMU's
`hw/char/cmsdk_apb_uart.c` model raises `TXIM` (TX interrupt) once
TX completes, exactly like real hardware. We don't currently wire
that IRQ; the driver polls instead.

The IRQ-driven design:

1. Driver registers a `UART0RX_IRQ` / `UART0TX_IRQ` / `UART0OVF_IRQ`
   handler (NVIC vector slots `5`, `6`, `7` on the mps2-an385 PAC).
2. `write` writes as many bytes as fit into the TX FIFO, then
   enables the TX-empty interrupt and `wfi`s.
3. The TX-empty IRQ handler clears the bit and returns; `wfi` resumes.
4. Loop until all bytes sent.

For RX, the driver registers the RX IRQ which fills the existing
`SerialPort::rx_buf` ring (currently filled by polled `read`).

This mirrors how `cmsdk_uart`-style drivers are written for real
hardware (TI Stellaris, NXP LPC, etc.).

## Work items

### 132.1 — Wire CMSDK UART IRQ vectors

**Files:**
- `packages/drivers/cmsdk-uart/src/lib.rs`
- `packages/boards/mps2-an385-pac/src/lib.rs` (verify NVIC vector names)
- `packages/boards/nros-board-mps2-an385/src/node.rs`
  (register handler, enable NVIC line)

**Tasks:**
- [ ] Identify the correct IRQ numbers for UART0 RX/TX/OVF on MPS2-AN385.
- [ ] Expose a `#[interrupt]` hook from `cmsdk-uart` that the board
      crate's `init_serial` connects.
- [ ] Add a per-port `TX_DONE_FLAG` so the handler can wake the
      writer without losing state.

### 132.2 — IRQ-driven `write`

**Files:**
- `packages/drivers/cmsdk-uart/src/lib.rs`

**Tasks:**
- [ ] Replace the busy `while self.tx_full() { spin_loop() }` with a
      `wfi`-on-TX-empty pattern.
- [ ] Confirm the TX-empty IRQ unmasks cleanly and re-masks after
      service (avoid IRQ storms).
- [ ] Verify byte ordering is preserved across the `wfi` boundary —
      the previous WFE experiment (2026-05-17, reverted) regressed
      zenoh init with `Unexpected Init flag` in zenohd logs.
- [ ] Bench `bytes/sec` on QEMU + on real MPS2-AN385 hardware
      (when available); should approach the 115200 baud line rate
      without burning the host CPU.

### 132.3 — IRQ-driven `read` (optional / follow-up)

**Files:**
- `packages/drivers/cmsdk-uart/src/lib.rs`

**Tasks:**
- [ ] RX IRQ handler that pushes into the existing
      `PortState::rx_buf` ring; replaces the current polled
      `read()` fill path.
- [ ] Verify against the COBS framing logic in
      `packages/zpico/zpico-serial/src/ffi.rs::_z_read_serial_internal`
      (which expects byte-by-byte arrival and a single 0x00
      delimiter terminator).

### 132.4 — RTIC interop check

**Files:**
- `examples/qemu-arm-baremetal/rust/zenoh/serial-*/src/main.rs`
- `examples/qemu-arm-baremetal/rust/zenoh/rtic-*/src/main.rs`

**Tasks:**
- [ ] Ensure non-RTIC serial examples gain the IRQ wiring
      automatically through the board crate.
- [ ] Verify the RTIC zenoh / DDS examples (which currently target
      LAN9118, not CMSDK UART) are unaffected.

### 132.5 — Test runner

**Files:**
- `packages/testing/nros-tests/tests/emulator.rs`
  (`test_qemu_serial_pubsub_e2e`)

**Tasks:**
- [ ] No code change expected; just verify the test passes under
      `QEMU_SYSTEM_ARM=build/qemu/bin/qemu-system-arm cargo nextest
      run -p nros-tests --test emulator --no-fail-fast --no-capture
      --retries 0 test_qemu_serial_pubsub_e2e`.
- [ ] Add the test to a "default" green-suite once it passes.

## Acceptance

- [ ] `test_qemu_serial_pubsub_e2e` passes against patched
      `qemu-system-arm`; listener receives ≥1 message, talker
      publishes ≥1 message.
- [ ] No regression in RTIC zenoh tests (`test_qemu_rtic_pubsub_e2e`,
      `test_qemu_rtic_service_e2e`, `test_qemu_rtic_action_e2e`,
      `test_qemu_rtic_mixed_priority_pubsub_e2e`).
- [ ] No regression in bare-metal DDS tests.

## Notes

### Why not "just add WFE to the write loop"

We tried this on 2026-05-17 (reverted):

```rust
while self.tx_full() {
    cortex_m::asm::wfe();
}
```

It does unblock the write hang — QEMU's main loop runs during
`wfe`. But the byte release timing changes enough that the zenoh
init handshake regresses: zenohd reports `Unexpected Init flag in
message` from
`zenoh-links/zenoh-link-serial/src/unicast.rs:164`, indicating the
COBS-framed Init frame from the guest arrives interleaved or
duplicated relative to zenohd's accept-side state machine. Likely
the guest re-sends Init while the previous attempt is still in
flight on the wire.

An IRQ-driven design fixes this because the TX path becomes
deterministic: bytes leave the FIFO at line rate, the writer
unblocks on the *actual* TX-empty event rather than on a
coarser-grained periodic WFE wake.

### Why not "arm SysTick + wfi_idle in init_hardware"

Also tried 2026-05-17 (reverted). The `enable_wfi_idle` hook only
fires inside `nros_baremetal_common::sleep_ms` and inside
`nros_smoltcp::do_poll` — neither covers the `CmsdkUart::write`
spin path. The hook would need to be threaded into the UART driver,
at which point we may as well do the proper IRQ-driven design.

### Why this isn't 127.D.3

Phase 127 is the post-Phase-124 failure triage. The 127.D.3 entry
recorded the bug, but the fix is a new driver feature, not a
narrow patch, so it earns its own phase per the project's roadmap
hygiene (`docs/roadmap/` is "phase doc per discrete chunk of
forward work"). 127.D.3 now links here.
