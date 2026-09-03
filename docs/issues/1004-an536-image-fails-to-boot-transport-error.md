---
id: 1004
title: "an536 island stopped booting reliably at pin d2a8955c5: `create_subscription` returns TransportError, sometimes preceded by a stack overflow"
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

## Why this is new

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

1. **Which stack overflows.** The banner prints an empty name, so the FreeRTOS
   overflow hook is reporting without a task. Fixing that reporting is a
   prerequisite for everything else — `an536-tasks.py` in the consumer repo
   dumps per-task stacks, but only for an image that gets far enough to attach.
2. **Bisect `2b03606ca..d2a8955c5`** (31 commits). The table above brackets it,
   and the two ends are known-good and known-bad on the same host.
3. **Only then** return to #0997.
