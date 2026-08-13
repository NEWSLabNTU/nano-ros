---
id: 552
title: "Zephyr Cortex-M C and C++ zenoh images branch to `PC=0` right after net init; Rust on the same board is fine"
status: open
type: bug
area: zephyr
related: [issue-0531, issue-0534, phase-337]
---

## Symptom

`zephyr_cortex_m_c_zenoh_pubsub_e2e` and `zephyr_cortex_m_cpp_zenoh_pubsub_e2e`
(board `mps2_an385`, tier-2 coordinate `zephyr-cortex-m,c,zenoh`) fault ~75 ms
after the network comes up, before a single sample is published:

```
[00:00:02.126,000] <inf> net_config: IPv4 address: 10.0.2.15
[00:00:02.201,000] <err> os: ***** USAGE FAULT *****
[00:00:02.201,000] <err> os:   Illegal use of the EPSR
[00:00:02.201,000] <err> os: r0/a1:  0x00000000  r1/a2:  0x00000000  r2/a3:  0x00000000
[00:00:02.201,000] <err> os: r3/a4:  0x00000000 r12/ip:  0x00000000 r14/lr:  0x00000000
[00:00:02.201,000] <err> os:  xpsr:  0x00000000
[00:00:02.201,000] <err> os: Faulting instruction address (r15/pc): 0x00000000
[00:00:02.201,000] <err> os: >>> ZEPHYR FATAL ERROR 35: Unknown error on CPU 0
```

The test then reports `Talker: expected at least 1 published messages, got 0`.

**`PC = 0` with every register zeroed is a call through a NULL function
pointer**, and "Illegal use of the EPSR" is what branching to address 0
produces on Cortex-M (no Thumb bit). It is not a stack overflow or a bad
dereference — control transferred to 0.

## What is established

* **Reproduces SOLO**, sequentially, on a freshly built fixture — not a
  parallel-sweep flake. Both the C and the C++ leaf, identically.
* ~~**Rust on the same board passes.**~~ **REFUTED 2026-08-13 — see below.**
  The language-split reasoning that followed from it (C/C++ registration seam,
  `nros_cpp_init` → `nros_app_register_backends`, a NULL slot) rests on a
  premise that does not hold, and should not be pursued on this basis.
* **native_sim is unaffected** — the same C/C++ examples pass there, so this is
  specific to the Cortex-M coordinate.

## What is REFUTED — it is not #531

The obvious suspect was `6da781901` (#531, "the Zephyr clock read zero on every
Cortex-M board under 60 MHz"). The story fit well enough to be worth testing:
before it, `nros_platform_clock_us()` returned a permanent 0 on this board, so
per its own commit message "the delta was 0 forever: periodic callbacks never
fired". Timers firing for the first time on a board where they had always been
dead is a plausible way to reach an unfilled callback slot.

Tested at HUNK level rather than argued: `git checkout 6da781901^ --
packages/platform/nros-platform-zephyr/src/platform.c`, rebuild the leaves
(all three ok), rerun.

| tree | result |
| --- | --- |
| HEAD | USAGE FAULT, both leaves |
| HEAD with the #531 hunk reverted | **USAGE FAULT, both leaves** (4 occurrences) |

So #531 is a bystander. Recorded because the coincidence is compelling and the
next person will suspect it too.

## Why it is only being seen now

Not because it is new — because the lane could not build. #531's own commit
message says it was **not runtime-verified**: "both Zephyr lanes fail to build
before reaching this file, in zpico-sys with `fatal error: version.h`". That
blocker was #534, fixed 2026-08-13. Fixing it let the Zephyr fixtures build and
the runtime cells actually execute.

**Whether these cells ever passed is NOT established.** The cells date from
phase-337 W2.c-f, the Zephyr build was blocked for part of 2026-08-13 by #534,
and no green run of this coordinate has been produced or found. Do not assume
this is a regression from a specific commit until someone has a passing revision
in hand.

## Where to look

* `nros_app_register_backends` / `nros_cpp_init` on the Cortex-M C and C++
  paths: which slot is still NULL when the first spin runs, and whether the
  registration call is reached at all before net init completes.
* The `FORCE_LINK` class (archived 0155/0163): a `#[no_mangle]` export present
  in the rlib but dropped from the staticlib by DCE gives a NULL slot with no
  link error, and its symptom is exactly an indirect call to 0.
* `nros-rmw-cffi` vtable slots are `Option<fn>` for C nullability, so an
  unfilled slot is representable and reaches the call site as 0.

## Not done

No bisect (no known-good revision to bisect against — see above), and no
inspection of the faulting image's symbol table to name the NULL slot. Both are
the obvious next steps; the second is cheap and should come first.


## CORRECTION 2026-08-13 — Rust faults too, and the split is not by language

The Rust image on `mps2_an385` faults **identically**: `PC = 0x00000000`, every
register zeroed, "Illegal use of the EPSR", ~25 ms after the C/C++ images do —
right after it prints `rust: rustapp::app_main: Waiting for messages`.

**The real split is whether a zenoh router is reachable**, measured on the same
fixture with the harness's own QEMU arguments:

| condition | runs | PC=0 faults | publishes |
| --- | --- | --- | --- |
| zenohd listening on 10600 | 2 | **2** | 0 |
| no router | 2 | **0** | — |

So the fault is in a path taken once the session actually connects, and it is
BOARD-WIDE rather than a C/C++ registration seam. The `nros_cpp_init` hypothesis
above cannot explain a Rust image that never goes through it.

**Why the original claim was plausible:** nothing ran the Rust cell. `matrix::CELLS`
has declared `(ZephyrQemuCortexM, Rust, Zenoh, Pubsub)` a `Runtime` cell since
phase-346 W3, but `zephyr_cortex_m_qemu.rs` only ever had C and C++ tests, so
"Rust builds and runs" could only have been a build observation or a run without
a router. `zephyr_cortex_m_rust_zenoh_pubsub_e2e` exists now and executes it.

**A second artifact that will mislead the next reader,** found the same way: the
two log orderings differ. For C/C++ the talker's `Publishing:` reaches the stream
BEFORE the boot banner, so those cells wait on the net line. Rust flushes the
other way — `net_config` first, publishes after — so waiting on the net line
kills the guest immediately after "Network ready", *before* the 500 ms timer, and
the run then looks exactly like a dead clock (issue 0531) when it is not. The
Rust cell waits on the talker line for that reason.
