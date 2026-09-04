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

### Cut further, 2026-09-04 — and the CONTROL corrected the framing

| variant | setup_complete | fault |
| --- | ---: | ---: |
| H — empty `register` AND empty `on_callback` | 0 | 1 |
| I — full talker, node `name = "listener"` | 0 | 1 |
| **listener, UNMODIFIED** | **1** | **0** |
| **listener, EMPTY `register`** | **1** | **0** |

The listener control is the one that matters. An empty `register` does NOT cause
the fault: the listener with the same empty body completes setup and spins. So
"an image that registers no entities faults" is REFUTED, and the earlier
contrast — which compared a non-empty listener against an empty talker — was not
like-for-like.

**Ruled out for the talker, each by a single-variable image:**

* the whole body of `register` (A–E)
* the `ENTITY_BOUNDS` static (F)
* the static IP (G)
* the node type's `on_callback` body (H)
* the node NAME (I)
* "no entities" as the trigger (the listener control)

Same fault address `0x42051d70` throughout.

### And yet the listener does not fault

Same board, same build system, same run conditions, a NON-empty `register`, and
it reaches its spin loop. After G, the differences left between the two leaves
are down to the node NAME (`"talker"` vs `"listener"`) and the node TYPE itself —
`Talker`'s `ExecutableNode` impl, its `State = i32`, its `on_callback`. Those are
what a further bisect should cut.

## Memory pressure REFUTED, and the constant PC is the finding (2026-09-04)

The leaf does not link without `ZPICO_MAX_QUERYABLES=2`, so "the image is over
budget and smashes its stack" was the obvious next hypothesis. It is wrong:

| `ZPICO_MAX_QUERYABLES` | fault | `mepc` |
| --- | ---: | --- |
| 1 | 1 | `0x732f7264` |
| 2 (shipped) | 1 | `0x732f7264` |
| 4 | 1 | `0x732f7264` |

Three builds with different session-struct sizes and therefore different
layouts, and **the faulting PC is byte-identical in all three**.

That rules out stack smashing, which would move with layout, and it rules out
memory pressure as the trigger. A constant wrong PC across differing builds
means the value is DATA the code reads deterministically — something loads
`0x732f7264` ("s/rd", printable text) from a fixed place and calls it.

So the shape is: **a code pointer read from a slot that holds string data**, not
random corruption. Candidates worth looking at first are the fn-pointer slots
this board actually installs — `nros_platform_esp32_qemu::register_log_writer`'s
writer slot, and the RMW backend registration
(`nros_rmw_zenoh::register()`) — because those are the places a `fn` value is
stored and later called on this target. That is a direction, NOT a diagnosis.

## Where to start

1. Resolve `0x42051d70` (the backtrace frame, which IS a code address unlike
   `mepc`) against `esp32_qemu_talker` with a RISC-V `addr2line`. The host
   `addr2line` in this checkout does not read it; the toolchain's
   `llvm-addr2line` should. This is now the single highest-value step: it names
   the caller that loaded the bad pointer.
2. Find what stores `0x732f7264` — search the ELF for that byte sequence and see
   which object it lands in. The PC is constant across builds, so the source is
   too.
3. ~~Bisect `register()`~~ — done, and it is none of it (table above).
4. ~~Suspect memory pressure~~ — refuted above.
