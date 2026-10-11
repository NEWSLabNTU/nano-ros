---
id: 1804
title: "The RTIC tick on MPS2-AN385 reprogrammed the monotonic clock's timer, so the clock jumped ~171.8 s per millisecond and every timeout fired on the next tick"
status: resolved
type: bug
severity: medium
area: [boards, platform, rtic, baremetal]
related: [0968, 0178]
phases: [289]
found: 2026-10-11
---

## Measured

`test_qemu_rtic_service_e2e` (issue 0968's last runtime cell) fails
intermittently. Solo, `--retries 0`, on fixtures built from this tree:

| build | runs | failed |
| --- | ---: | ---: |
| `main` (`66104bdd9c`), two batches | 21 + 20 | 4 + 1 |
| this fix, two batches | 20 + 20 | 0 + 0 |

The sibling cells (`test_qemu_rtic_{pubsub,mixed_priority_pubsub,action}_e2e`)
pass with the fix too (4 of 4, solo).

The failure is the firmware's own `zpico Session -> ConnectionFailed`. The
router log of a failing run shows that the router is fine. It accepts the guest's TCP
connection, then gets a SECOND `InitSyn` 23 ms later where it expects
the `OpenSyn` (`Received invalid message instead of an OpenSyn`). So the guest
gave up on the handshake and started it again almost at once.

The log timestamps on the guest gave the cause away. The service server prints
its first `nros` line at **`[  687.19]`**, about a second after boot.
Throughout the run every timer callback reports `contract violation:
timer-overrun-runtime measured=171`, and the jitter check reports
`measured=171798066`. 2^32 / 25 MHz = 171.8 s, so the clock is counting
whole 32-bit wraps that never happened.

With the fix, the same line prints at `[    0.003438]`, and `measured=171` appears
0 times in 40 logs. Without it, it appears in every passing log of the second
reverted batch (19 of 19). That batch's one failing log shows the same jump in its
timestamps (`3264.17`, `3607.77`).

There is a second symptom, and it does not depend on timing. The client issues ONE request.
The server handled **1** request in every fixed run (40 of 40). With the defect it
handled between 6 and 100: each spurious timeout made the client
retransmit. (One fixed run logged `zpico Generic -> ConnectionFailed` at
`[1.025]`. That was AFTER `Result of add_two_ints: 5`, during teardown, and the cell passed.)

## Cause

Both users of CMSDK **TIMER0** on MPS2-AN385 program it as if they owned it:

* `nros_platform_mps2_an385::clock` is the monotonic clock. It runs TIMER0
  free-running at RELOAD `0xFFFF_FFFF`. It counts a wrap whenever
  `VALUE` goes up, and adds 2^32 ticks for each one.
* `nros-board-mps2-an385`'s RTIC entry (phase-289, issue 0178) arms TIMER0 as
  its 1 ms `wfi` wake tick, with RELOAD 25 000. The macro binds the handler with
  `#[task(binds = TIMER0)]`.

Once the tick is armed, every 1 ms reload reads as a full wrap. So
`clock_now_ms()` jumps ~171 800 ms at each tick boundary and is accurate only
between two of them. A deadline such as `now - start > SOCKET_TIMEOUT_MS`
(TCP connect, `read_exact` and `send` in `nros-smoltcp`'s platform macro) holds
only until the next tick. A zenoh handshake whose reply arrives in a later tick
fails. Under `-icount shift=auto` that depends on host load, which is why the
cell passes most of the time. It is also why phase-289 could
report the RTIC lanes green.

The direct-exec (non-RTIC) MPS2 entry never arms a tick, so it was not
affected. That includes the C/C++ bare-metal images from issue 1512.

## Fixed

The tick moved to the CMSDK **dual timer** (`0x4000_2000`, IRQ 10, `binds =
DUALTIMER`). TIMER1 was not an option: `sporadic_timer` already owns it and
defines the `TIMER1` vector, and linking the tick there fails with
`duplicate symbol: TIMER1`.

The class is "a peripheral with two owners". So both owners now export their
timer base: `clock::CLOCK_TIMER_BASE` and
`sporadic_timer::SPORADIC_TIMER_BASE`. The RTIC tick has a `const` assertion
that it is neither, so a future change that puts it back on either timer fails
to compile.

## Not covered

The macro's `binds = DUALTIMER` (`rtic_board_spec_for` in nros-macros) and the
board's `CMSDK_DUALTIMER_BASE` are two crates naming one peripheral. A proc
macro cannot read a board constant, so the pairing is enforced by comments, not by a check. If
they disagree, the tick IRQ is never acknowledged. That fails loudly, as an IRQ
storm or a dead `wfi` wake, rather than silently.
