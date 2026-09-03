---
id: 1004
title: "an536 island hangs nondeterministically at controller construction — three signatures, both pins, no bracket"
status: open
area: [rmw, platform, embedded]
severity: high
related: [0997, 1000, phase-177]
---

# The same image that ran for minutes this morning now halts at boot

## What happens

Autoware Safety Island, `freertos-an536` on QEMU `mps3-an536`, nano-ros pin
`d2a8955c5`:

```
[INFO] nros: Wall clock set from SNTP
[INFO] nros: Starting Controller Node...
[INFO] nros: Actuation Safety Island is Live
[nros] FATAL: ComponentNode "controller" failed at create_subscription (code=-100) — halting boot
```

`-100` is `nros::Result::TransportError` (`nros-cpp/include/nros/result.hpp:70`).

One run in three failed differently and earlier, before any component came up:

```
[INFO] nros: Wall clock set from SNTP
[INFO] nros: Starting Controller Node...
*** STACK OVERFLOW:  ***
```

with an EMPTY task name in the banner.

Those two are almost certainly one defect. `nros-board-freertos`'s own
`lwipopts.h` records the same pairing from phase 177.26, in as many words:

> `recvUC` overflowed on the first real ROS payload (a 13 KiB Autoware
> trajectory) with `*** STACK OVERFLOW: recvUC ***`, and — because the overflow
> lands in the adjacent heap — the SAME image also failed at
> `create_subscription` with a bad-free heap_4 assert when it booted into an
> already-populated graph.

So: a stack overflow corrupts adjacent heap, and the corruption presents later
as a `create_subscription` failure. The difference from 177.26 is that the
banner names no task, so which stack is overflowing is not yet known.

## CORRECTED — there is no bracket, and it is not a regression in those 31 commits

The table below was built from one or two observations per pin. Re-tested with a
scripted predicate that boots each build THREE times and calls a pin bad if any
attempt fails:

| pin | 3-attempt result |
| --- | --- |
| `d2a8955c5` (was "bad") | **BAD** — stack overflow on attempt 1 |
| `2b03606ca` (was "good") | **BAD** — attempt 2 |

**Both ends fail.** So the bracket in the original table does not hold, the
"regression inside 31 commits" framing is unsupported, and the correlation drawn
with island tracing being absent is unsupported with it. A bisect was prepared
and abandoned: with a noisy predicate it would have named an innocent commit with
confidence, which is the failure mode `AGENTS.md` records for issue 0268 — "a
first-bad that cannot plausibly cause the symptom means the verdicts tracked a
confounder".

What the retest also showed is that the run I called good was good: attempt 1 at
`2b03606ca` produced 9440 lines and ran the full 75 s. Attempt 2 produced NINE:

```
Network ready
[INFO] nros: ARM - Autoware: Actuation Safety Island
[INFO] nros: Wall clock set from SNTP
[INFO] nros: Starting Controller Node...
qemu-system-arm: terminating on signal 15 (timeout)
```

It reaches SNTP, starts the controller, and then hangs silently — no overflow
banner, no FATAL, no further output. That is a THIRD signature alongside the
stack overflow and the `create_subscription -100`, and all three land in the same
window: immediately after `Starting Controller Node`.

So the honest description is one nondeterministic hang at controller
construction, presenting three ways, on both pins — not a version regression.
Roughly one run in three on this host.

**Method note, because it caused the wrong issue to be filed:** a single boot
attempt is not evidence about a ~1-in-3 failure. The original bracket came from
1–2 samples per pin and was wrong in the direction that felt most explanatory.
Any future claim here needs a repeat count stated with it.

Environment checked and NOT the cause: the SNTP server is running throughout
(`sntp-server.py --bind 192.0.3.1`), and the failing boot reaches
`Wall clock set from SNTP`, so it is not a missing time source.

## Why this looked new (superseded by the correction above)

The same tree booted this image repeatedly earlier the same day. Sequence, all
on one host with one tap and one QEMU (`11.0.0-nros2`):

| pin | boots? | evidence |
| --- | --- | --- |
| `2b03606ca` | yes | full 7-size delivery sweep completed; later runs booted and ran for minutes |
| `d2a8955c5` | yes, WITH island tracing | ran 161 s past the SPDP stall, logging throughout |
| `d2a8955c5` | **no**, without tracing | 3 of 3 runs failed: two `create_subscription -100`, one stack overflow |

The last row is the current committed pin.

Two things are worth separating, because it is tempting to collapse them:

* **It is not the instrumentation.** The failure reproduces with the local
  Cyclone counters fully reverted (`git checkout` of `q_xevent.c`, verified
  clean) and with no `<Tracing>` block in `kEmbeddedCycloneConfig`.
* **It correlates with tracing being ABSENT.** Every long successful run at this
  pin had island-side discovery tracing enabled; every failure has it off.
  Tracing over semihosted stdout is slow, so this has the shape of a timing
  window that logging happens to close — which is a Heisenbug, and is why
  "add tracing to see it" is not a viable next step on its own.

## Why it matters beyond itself

It blocks [#0997](0997-island-announces-spdp-once-then-lease-expires.md) and
[#1000](1000-spdp-periodic-event-orphaned-by-handler-early-return.md). Neither
can be confirmed or refuted while the image does not reach a steady state
deterministically, and #1000's proposed mechanism is already contradicted by the
one counter read that did complete (`nros_dbg_spdp_unknown_guid = 0`,
`nros_dbg_spdp_no_writer = 0` across three handler invocations).

It also means the an536 delivery numbers taken today cannot be trusted as a
before/after of anything, which is recorded in #0997 for the same reason.

## Reproduce

```
# ASI at 3ebde20 or later, nano-ros pin d2a8955c5
sudo ip link set tap1 up            # 192.0.3.1/24, netem 100mbit
ASI_RX_COUNTERS=1 NROS_DOMAIN_ID=2 ./build.sh --platform freertos-an536
# boot with -net tap,ifname=tap1 and watch for the FATAL line
```

A gated runner is what made this visible rather than being mistaken for a
measurement — it requires the island to boot AND a publisher to match AND the
match to drop before it will call a run data, and it named which gate failed
each time. A run that fails to boot is otherwise easy to read as "the scenario
reproduced and nothing arrived", which is exactly the confusion that cost a day
here.

## What to establish first

1. **Attach to a hung instance.** The silent hang is the most tractable of the
   three signatures: it reaches `Starting Controller Node` and stops, so the
   image is alive and gdb can be attached to ask which task is stuck and on
   what. `an536-blocked-on.py` in the consumer repo answers exactly that, and it
   is what identified the condvar waiters in #0997.
2. **Which stack overflows.** The banner prints an EMPTY task name, so the
   FreeRTOS overflow hook reports without one. That reporting needs fixing
   regardless, because it is the difference between "some stack" and a fix.
3. **NOT a bisect.** Both ends fail; there is nothing to bisect until the
   failure is deterministic or the predicate is made reliable.
4. **Only then** return to #0997.
