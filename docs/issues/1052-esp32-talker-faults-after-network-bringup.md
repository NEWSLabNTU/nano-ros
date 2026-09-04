---
id: 1052
title: "The esp32-qemu talker takes an instruction-access fault right after network bring-up, with a return address made of ASCII"
status: open
area: rmw, boards
severity: high
found: 2026-09-04
related: [0968, 1048, 0291]
---

# The PC is a string, not a function

## What the image prints

Run alone under QEMU with a zenoh router up:

```
Initializing OpenETH...
  IP: 10.0.2.51/24
Ethernet ready.

Exception 'Instruction access fault' mepc=0x732f7264, mtval=0x732f7264
TrapFrame { ra: 1932489317, t0: 9, t1: 0, t2: 1070320272, … }

Backtrace:
0x42051d70
```

It never reaches `Application setup complete`, so `register()` — one publisher,
one timer — does not finish.

## The registers are ASCII, which is the whole lead

```
mepc = mtval = 0x732f7264  ->  b"s/rd"
ra              0x732f7265  ->  b"s/re"
```

Execution branched to an address whose bytes are printable source-path text, and
`ra` holds the adjacent value. Strings of that shape in this binary are
`file!()` paths in rodata — `zenoh-pico/src/collections/refcount.c` and
`src/iter/adapters/rev.rs` both contain the `s/re` run.

So this is a corrupted return address or function pointer, not a missing symbol
or an unmapped fetch: something wrote text over a code pointer, or a pointer was
read from the wrong place. `refcount.c` is zenoh-pico's reference counting,
which is a plausible neighbourhood for it and is NOT a claim that it is the
culprit.

## Not caused by the logging fix, and not new

The same `Instruction access fault` appears twice in the tier-2 capture from
BEFORE issue 1048's fix landed
(`test_esp32_to_native`, run at the original tree state). The fix changed what
the board can print; it did not change this.

**It was invisible for the same reason everything else was.** With no log sink
installed (issue 1048) the image could not say where it stopped, so this read as
"the talker never printed `Publishing:`" — a missing marker rather than a crash.
The two failures were stacked, and the logging one had to come off first.

## The contrast that scopes it

The LISTENER, same board, same build, same run conditions, does not fault. It
reaches `Application setup complete`, enters its spin loop, and then reports
`zpico Generic -> ConnectionFailed` repeatedly — a different problem, and one it
survives.

So this is specific to the talker's path. The difference in `register()` is a
publisher plus a timer that `publishes_entity` against it, where the listener
creates one subscription.

## What this blocks

`test_esp32_talker_listener_e2e` and `test_esp32_to_native` in
[issue 0968](0968-tier2-runtime-failures-unreproduced.md). Both wait on the
talker's `Publishing:` marker, which the image cannot reach.

## BISECTED 2026-09-04 — it is NOT in `register()`

Six images, one variable each, every one built with the row's env and run under
QEMU with a router up. `setup_complete` is `Application setup complete`;
`fault` is the instruction-access fault.

| variant | setup_complete | fault |
| --- | ---: | ---: |
| A — `create_node` only | 0 | 1 |
| B — + publisher | 0 | 1 |
| C — + timer | 0 | 1 |
| D — full (`publishes_entity`) | 0 | 1 |
| **E — `register` does NOTHING (`Ok(())`)** | **0** | **1** |
| F — E, plus `ENTITY_BOUNDS::exact(0,0,0,0,0)` | 0 | 1 |
| G — full, but with the LISTENER's `ip = 10.0.2.51` | 0 | 1 |

**The fault address is `0x42051d70` in every one.**

So it is none of: the publisher, the timer, `publishes_entity`, the entity-bounds
static, or the static IP. An EMPTY `register` faults identically, which puts it
outside the node's own registration code entirely — in the image's startup path,
before or around `Executor::open`.

### And yet the listener does not fault

Same board, same build system, same run conditions, a NON-empty `register`, and
it reaches its spin loop. After G, the differences left between the two leaves
are down to the node NAME (`"talker"` vs `"listener"`) and the node TYPE itself —
`Talker`'s `ExecutableNode` impl, its `State = i32`, its `on_callback`. Those are
what a further bisect should cut.

## Where to start

1. Resolve `0x42051d70` (the backtrace frame, which IS a code address unlike
   `mepc`) against `esp32_qemu_talker` with a RISC-V `addr2line`. The host
   `addr2line` in this checkout does not read it; the toolchain's
   `llvm-addr2line` should.
2. Bisect `register()`: publisher only, then timer only. The fault is before
   `Application setup complete`, so it is inside those three calls.
3. Note that zenoh-pico is pinned 1.7.2 (issue 0291) and the esp32 image is the
   RAM-tightest in the tree — a corrupted pointer here may be an overflow of
   something sized by a knob rather than a logic bug.
