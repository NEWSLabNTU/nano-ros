# Phase 132 — CMSDK UART IRQ scaffolding + serial-only build fix

**Goal (rescoped).** Land the structural scaffolding that future
IRQ-driven CMSDK UART work needs:

1. ISR servicing entry points in the `cmsdk-uart` driver
   (`handle_tx_irq`, `handle_rx_irq`).
2. Board-crate `#[interrupt]` forwarders + opt-in `mps2-an385-pac`
   dep gated on the `serial` feature.
3. `ZPICO_NO_SMOLTCP=1` env opt-out in `zpico-sys`'s `build.rs` so
   serial-only embedded builds link cleanly (Phase 128 retired the
   per-transport Cargo features that used to gate this).

The original ambition — replace the busy-spin TX with an IRQ +
`wfi` design so `test_qemu_serial_pubsub_e2e` passes — turned out
to be the **wrong fix for the actual symptom**. Diagnosis under
"Root cause (revised)" below.

**Status.** Scaffolding landed (commits `02f62291`, `b23acf84`).
IRQ-driven `write` deferred. Original 127.D.3 acceptance test
still red, for a different reason now tracked separately.

**Spun out from** [`phase-127-remaining-failure-groups.md`](phase-127-remaining-failure-groups.md)
`127.D.3` on 2026-05-17 after the D.1 / D.2 / D.4 portions of 127.D
closed.

**Priority.** Low. The IRQ scaffolding is precondition work for
any future serial-transport latency / power optimisation; the
real `test_qemu_serial_pubsub_e2e` blocker is a separate
init-handshake regression filed as a follow-up phase.

**Depends on.** None. Patched QEMU (`third-party/qemu/qemu` +
`third-party/qemu/patches/`) is in place from 127.A/D.

---

## Root cause (revised)

After landing the IRQ wiring + reverting to busy-spin write, the
test still fails — both talker AND listener freeze at
`Zenoh locator: serial/UART_0...` **before any UART byte goes
out**. They stall inside `Executor::open` →
`_z_open_serial_from_dev` → `connect_serial`'s
Init / InitAck exchange (see
`packages/zpico/zpico-serial/src/ffi.rs:138`).

The original 127.D.3 hang was downstream of session init
("Publishing messages over serial..." then FIFO-fill on the
second publish). We never reach that point now because session
init itself hangs.

Verified by running the OLD May-15 binary side-by-side: it gets
past init and reaches the original FIFO-fill spin. The
regression that introduced the init hang landed between Phase
128 and the current main — likely Phase 128.E.1's deletion of
the per-transport Cargo features (`link-tcp` / `link-udp` /
`link-serial`) reshaping the `LinkFeatures` defaults that
`generate_config_header` writes into `zenoh_generic_config.h`,
which the embedded `_z_open_serial` path consults.

QEMU CMSDK study (`third-party/qemu/qemu/hw/char/cmsdk-apb-uart.c`
+ `chardev/char.c:329`): `null_chr_write` and PTY chardevs return
the byte length immediately; `uart_write` calls `uart_transmit`
synchronously inside the MMIO handler; `buffer_drained` clears
`TXFULL` and (if `CTRL_TX_INTEN` is set) raises `INTSTATUS_TX`
before the guest's next instruction. Busy-spin TX is therefore
fundamentally adequate for QEMU — the IRQ-driven design adds no
correctness benefit, only the option to `wfi` and let other
ISRs run while the FIFO drains. Real hardware would benefit
more; QEMU emulation does not.

Conclusion: 132's IRQ-driven write was solving a non-problem on
QEMU. The structural pieces (ISR symbol, PAC dep, ZPICO_NO_SMOLTCP)
stay — they're useful regardless — but the IRQ swap-in is
no longer scheduled.

---

## Work items

- [x] **132.1 — Wire CMSDK UART IRQ vector forwarders.**
  - `cmsdk-uart` exports `handle_tx_irq(base)` /
    `handle_rx_irq(base)` ISR helpers + the `INTSTATUS` /
    `CTRL_TX_INT_EN` / `CTRL_RX_INT_EN` register bit constants.
  - `nros-board-mps2-an385`'s `node.rs` ships
    `#[interrupt] fn UARTTX0()` that forwards to
    `cmsdk_uart::handle_tx_irq(UART0_BASE)`. Vector defined; ELF
    `nm` shows it in the `.vector_table.interrupts` slot for IRQ
    line 1 (UARTTX0).
  - New opt-in `mps2-an385-pac` dep on the board crate, gated by
    the `serial` Cargo feature.
  - NVIC line stays masked today (no caller needs it); structural
    scaffold present.

- [x] **132.bonus — `ZPICO_NO_SMOLTCP=1` env opt-out in
  `zpico-sys/build.rs`.** Skips the `ZPICO_SMOLTCP` define + the
  `smoltcp_init` / `smoltcp_cleanup` link refs that would
  otherwise pull undefined symbols into serial-only embedded
  binaries. Wired in `serial-talker` and `serial-listener`
  `.cargo/config.toml [env]` blocks. Unblocks any future
  serial-only embedded testing.

- [x] **132.0 — Renumber duplicate Phase 132 doc.**
  `phase-132-wake-callback-cortex-m3.md` → Phase 141 (commit
  `7f7beab6`). Internal section headers updated 131.A/B/C/D →
  141.A/B/C/D.

- [ ] **132.2 (descoped) — IRQ-driven `write`.** Deferred. QEMU
  study shows busy-spin is adequate for the emulator; the IRQ
  swap-in would only help real hardware. File as a separate
  enhancement if real-hardware serial-rate latency becomes a
  requirement.

- [ ] **132.3 (descoped) — `test_qemu_serial_pubsub_e2e` green.**
  Moved out of 132's scope. The init-handshake regression that
  blocks the test today is a separate bug class (zenoh-pico
  bare-metal `connect_serial` post-Phase-128). File as a
  follow-up phase ("zenoh-pico bare-metal serial Init/InitAck
  regression").

---

## Acceptance (rescoped)

- [x] `cmsdk-uart` ships `handle_tx_irq` / `handle_rx_irq`
  helpers + IRQ bit constants. Board crates can wire the ISR in
  one `#[interrupt]` forward.
- [x] `nros-board-mps2-an385` builds clean under `serial,rmw-zenoh`
  on `thumbv7m-none-eabi` with the ISR symbol present in the
  vector table.
- [x] `serial-talker` / `serial-listener` link clean on
  `thumbv7m-none-eabi` without `smoltcp_init` / `smoltcp_cleanup`
  undefined-reference errors.
- [ ] (Stretch, moved to follow-up phase.) `test_qemu_serial_pubsub_e2e`
  passes against the patched `qemu-system-arm`.

---

## Notes

- **Why the original WFE approach failed** (preserved from earlier
  revision for the next reviewer): the 2026-05-17 WFE attempt
  changed byte timing enough to break the zenoh init handshake
  with `Unexpected Init flag in message` from zenohd. We've since
  established the handshake is broken before any UART write even
  occurs in the post-Phase-128 build, so the WFE diagnosis was
  itself a misattribution — the handshake was breaking inside
  zenoh-pico's bare-metal session-init path, not at the wire.
- **Why this isn't 127.D.3.** Phase 127 was the post-Phase-124
  failure triage. The 127.D.3 entry recorded the original FIFO-
  fill hang. The current failure mode is upstream of that — file
  the init-handshake regression separately so 127.D.3 stays as
  the historical record of the FIFO-fill bug.
- **QEMU CMSDK model is synchronous on TX-drain for null +
  PTY chardevs** (`null_chr_write` returns the full length
  immediately; `uart_transmit` runs sync inside the MMIO
  handler). Busy-spin TX is functionally adequate. Real hardware
  with a finite-speed wire is where IRQ-driven TX would actually
  pay off.
