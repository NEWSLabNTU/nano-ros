# Phase 142 — zenoh-pico Bare-Metal Serial Init/InitAck Regression

> **ARCHIVED — RESOLVED.** Fix: drop `target_os = "none"` from the
> linkme allow-list in `packages/core/nros-rmw-cffi/src/section.rs`.
> Root cause: `cortex_m_rt`'s link script doesn't provide
> `__start_/__stop_` section anchors for linkme in a shape that
> lets `RMW_INIT_ENTRIES.iter()` terminate — bare-metal binaries
> hang inside `Executor::open` before `_z_open_session` is ever
> called. Bare-metal firmware was already using the explicit
> `nros_rmw_<x>::register()` call (Phase 104.A pattern), so the
> linkme path is unused on bare-metal; falling into the stub path
> (empty slice + walker returns 0) is the correct behaviour.
>
> Verified: `cargo nextest run -p nros-tests --test emulator
> --no-fail-fast --retries 0 test_qemu_serial_pubsub_e2e` → PASS
> in 8.9 s.

**Goal.** Make `test_qemu_serial_pubsub_e2e` pass against the
patched MPS2-AN385 QEMU. The test today hangs both talker and
listener at `Zenoh locator: serial/UART_0...` inside
`Executor::open` → `_z_open_serial_from_dev` → `connect_serial`'s
Init / InitAck exchange.

**Status.** Not started. Scoped + study landed here so the next
session can dive in with a fix-path roadmap instead of
re-discovering the failure mode.

**Priority.** P2 — same scope as the original Phase 132 acceptance
target, separated because the IRQ-driven UART approach Phase 132
was built around is the wrong fix.

**Depends on.** Phase 132 scaffolding (ISR symbol export +
`ZPICO_NO_SMOLTCP` env, both landed in commit `02f62291`).

**Related.** Phase 132 (the original misattribution + scaffolding),
Phase 128 (likely regression source — deleted per-transport Cargo
features), Phase 129.D (deleted `zpico-platform-shim`),
Phase 134 (canonical-header reshuffle).

---

## Failure mode (reproduced)

`QEMU_SYSTEM_ARM=/usr/bin/qemu-system-arm cargo nextest run -p
nros-tests --test emulator test_qemu_serial_pubsub_e2e`:

Both talker and listener stop at:

```
Zenoh locator: serial/UART_0#baudrate=115200
```

and never emit the post-`Executor::open` lines
("Declaring publisher", "Subscribing to /chatter").

zenohd's trace log (via `ZENOHD_LOG=trace`) confirms:

```
Ready to accept Serial connections on: ".../serial-listener-zenohd"
Ready to accept Serial connections on: ".../serial-talker-zenohd"
Waiting for connection
Waiting for connection
```

**zero bytes** reach zenohd over the socat-bridged PTYs.

## Where the hang lives

Inside `packages/zpico/zpico-serial/src/ffi.rs:138::connect_serial`:

```rust
for _ in 0..SERIAL_CONNECT_MAX_ATTEMPTS {
    let port = unsafe { get_port(index) }?;
    let written = port.write(&init_frame[..init_frame_len]);
    if written != init_frame_len { return Z_ERR_GENERIC; }
    for _ in 0..200 {
        match read_handshake_frame(index) {
            ...
        }
    }
}
```

Either `port.write` is hanging (busy-spin in `CmsdkUart::write`
on a full FIFO that never drains for some reason), or the write
completes but the read loop times out 200×10 ms × retries with
no response from zenohd.

Phase 132 instrumented from the QEMU side (no bytes received).
Combined with the host PTY chain (`-chardev serial,path=symlink`
→ socat PTY pair → zenohd), the bytes are getting **lost
somewhere on the QEMU → socat → zenohd path**. The most likely
cause is QEMU's `-chardev serial,path=...` opening the symlink
with termios flags that mismatch the PTY's master side, causing
write-side data to never reach socat's other half.

## Pre-Phase-128 sanity baseline

The May-15 commit (`fbf5d949`) shipped a working
`qemu-serial-talker` binary that reached the original 127.D.3 hang
("Publishing messages over serial..." → spin on FIFO fill). The
binary survives in `examples/qemu-arm-baremetal/rust/zenoh/serial-
talker/target/thumbv7m-none-eabi/release/qemu-serial-talker` from
the May-15 build. Useful as a "known-passing past `connect_serial`"
reference — the regression is between `fbf5d949` and current main.

## Fix-path roadmap

### Layer 1 — guest-side diagnostic instrumentation (cheap, do first)

Add `cortex_m_semihosting::hprintln!` at four points inside
`connect_serial`:

1. Right after `_z_serial_msg_serialize` returns —
   `eprintln!("Init frame ready, len={}", init_frame_len);`
2. Before `port.write` — `eprintln!("Calling port.write({})", len);`
3. After `port.write` — `eprintln!("port.write returned {}", written);`
4. Inside the read loop, every 50 iterations —
   `eprintln!("Polling read attempt {}", i);`

Distinguishes:
- **Hang at write** (busy-spin FIFO problem). Write call doesn't
  return.
- **Write completes, no read response** (host-side bridge or
  zenohd problem). Reads return 0 forever.
- **Init frame serialize fails** (length 0 or `usize::MAX`).
  Phase 134's canonical-header changes could have flipped a
  Z_FEATURE flag that makes `_z_serial_msg_serialize` a stub.

### Layer 2 — standalone host-side bridge sanity (medium)

Strip zenoh-pico out of the loop. Replace the QEMU binary with a
minimal Rust talker that:

1. Initialises CMSDK UART (existing `init_serial`).
2. Sleeps 1 s for zenohd to be ready.
3. Writes a hardcoded 9-byte COBS-framed Init frame to UART
   (literal byte sequence captured from a working zenoh-pico
   POSIX talker).
4. Polls UART read for 5 s; if any bytes arrive, semihosting-print
   them.

If zenohd accepts the hardcoded frame and responds, the host
chain (QEMU `-chardev serial` ↔ socat ↔ zenohd) works and the
bug is in zenoh-pico's `connect_serial`. If zenohd never sees
the frame, the host chain is broken and the fix is on the test
fixture side (different `-chardev` flags, replace `serial` with
`pipe`/`pty`, or a non-blocking `O_NONBLOCK` mismatch).

### Layer 3 — bisect Phase 128/129/134 (longer)

Walk the May 15 → current main commit list and rebuild
`qemu-serial-talker` at each step. The five candidates that
touched the bare-metal serial path:

- `198b3977` phase-128.E.0 — auto-enable tcp+udp-unicast on POSIX
- `8b38350e` phase-128.E.1-3 — delete link-tcp/udp/serial features
- `5cb0efdc` phase-128.D.3 — ship `platform_aliases.c`
- `384f5e12` + `35b374ba` phase-129.A.4 / D — aliases default-on,
  shim suppressed
- `d421f275` phase-129.D — delete `zpico-platform-shim` entirely

The most likely culprit is `8b38350e` (deleted `link-tcp/udp/serial`
features). `LinkFeatures::from_env()` now hardcodes
`tcp = udp_unicast = udp_multicast = serial = true`, which means
the embedded build pulls every transport's vendor code. That
might satisfy some `Z_FEATURE_LINK_*` `#if` guard inside zenoh-pico
that the serial Init frame logic depends on.

The Phase 132 `ZPICO_NO_SMOLTCP` env opt-out is a workaround for
one symptom of that change (broken `smoltcp_init` link refs in
serial-only builds); the Init handshake failure is likely a
sibling symptom.

### Layer 4 — fix and verify

Land the actual fix. Acceptance:

- [ ] `test_qemu_serial_pubsub_e2e` passes in `just qemu test`.
- [ ] `serial-talker` + `serial-listener` standalone QEMU runs
      with patched `qemu-system-arm` reach
      "Publisher declared" / "Subscriber declared" within 5 s of
      `Executor::open`.
- [ ] zenohd trace log shows InitSyn + InitAck + KeepAlive frames
      flowing.
- [ ] No regression in RTIC zenoh tests
      (`test_qemu_rtic_pubsub_e2e`, …) which already pass.

---

## Notes

- The Phase 132 IRQ scaffolding (ISR symbol + opt-in PAC) stays
  even after this phase closes — useful for any future
  real-hardware serial work where the IRQ-driven write actually
  pays off (QEMU emulation is synchronous-drain so busy-spin is
  fine, per the QEMU CMSDK study in
  `archived/phase-132-cmsdk-uart-irq-driven.md`).
- The original 127.D.3 hang ("Publishing messages over serial..."
  FIFO-fill spin) is downstream of this init bug. Even if we
  switched to busy-spin write + got past `connect_serial`, we'd
  still hit the FIFO-fill hang during repeated publishes. A
  complete fix needs both: (a) connect_serial returns successfully
  (this phase), (b) repeated `publish()` doesn't spin forever on
  a 1-deep CMSDK FIFO. Once (a) is fixed, (b) is the original
  127.D.3 — which would benefit from an actual yield in the
  busy-spin (a single `wfi` per spin iteration, contingent on
  *some* IRQ source being unmasked so the CPU wakes; the QEMU
  CMSDK `g_source` callback only fires when the guest yields, so
  bare `wfi` without an unmasked source hangs forever).
- This phase replaces what Phase 132 was originally chartered to
  do but ended up not solving.
