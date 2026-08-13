---
id: 552
title: "Zephyr Cortex-M C/C++ images overflow the main stack with the executor inline storage, faulting as `PC=0`"
status: resolved
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

## ROOT CAUSE — a main-thread STACK OVERFLOW, not a NULL pointer

**The `PC = 0` reading in this issue was wrong.** It is a consequence, not the
cause, and the "call through a NULL function pointer" framing above would have
sent the next reader into the registration/DCE seam for nothing.

Recovered by dumping the exception frame under gdb (breakpoint on
`z_arm_fault`, which receives `msp`/`psp`), with a zenoh router listening on the
port the image has BAKED IN — `tcp/10.0.2.2:10700`, allocated by the matrix's
`port_of`, not a literal. Without that listener the image exits cleanly via
`run_components failed rc=-100` and never reaches the fault at all, which is a
different code path and cost two invalid runs before I noticed.

Both stack pointers were garbage:

| | value | lands in |
| --- | --- | --- |
| PSP | `0x2001E5A0` | `z_idle_threads` — a thread control block, not a stack |
| MSP | `0x2001E4E0` | `g_sessions` — the zenoh session array |

So the CPU stacked its exception frame into arbitrary data, every stacked
register read back zero, and the reported `PC` was zero along with them.

Rebuilding the same image with `CONFIG_HW_STACK_PROTECTION=y` collapsed it to
one line:

```
***** MPU FAULT *****  Data Access Violation   MMFAR Address: 0x20075a00
>>> ZEPHYR FATAL ERROR 2: Stack overflow on CPU 0
Current thread: 0x2001e898 (main)
```

The faulting PC symbolises to `__aeabi_memset4` reached from
`nros_node::executor::spin` — the bulk zeroing of the executor's INLINE STORAGE,
which the C/C++ entry places on the main thread stack. This build sizes it at
`NROS_EXECUTOR_SIZE = 88192` bytes against a `CONFIG_MAIN_STACK_SIZE` of
`16384`: a 5.4x overflow.

That is also why RUST on the same board passes — its entry does not put the
executor there — and why native_sim is unaffected, since the POSIX arch main
thread does not have a 16 KB stack. The language split that looked like a
registration-seam clue was the allocation site all along.

Same class as FreeRTOS's 64 KB `APP_TASK_STACK` (platform-implementation-notes:
"inline executor arena on stack"), for the same reason.

## Fix

`cmake/zephyr/mps2-an385.conf`:

* `CONFIG_MAIN_STACK_SIZE` 16384 -> 131072. ~40 KB of headroom over the current
  executor size, on a board with 4 MB of SRAM — the file already notes it is
  "not a tight board".
* `CONFIG_HW_STACK_PROTECTION=y` + `CONFIG_THREAD_NAME=y` kept PERMANENTLY, not
  reverted with the rest of the diagnostic scaffolding. A guard region costs an
  MPU slot and a little RAM; the alternative is what this issue documents — a
  fault whose registers are all zero, whose first plausible reading is wrong,
  and which cost a gdb session to attribute. The next overflow on this board
  names its own thread.

Verified: `zephyr_cortex_m_c_zenoh_pubsub_e2e` and
`zephyr_cortex_m_cpp_zenoh_pubsub_e2e` both PASS (4.4 s / 3.4 s), zero USAGE
FAULTs and zero stack overflows, on freshly rebuilt fixtures.

## Not addressed here

The executor is ~86 KB of stack for any C/C++ entry, and every board pays it out
of a hand-picked constant. `NROS_EXECUTOR_SIZE` is generated and known at build
time, so a board conf could be CHECKED against it rather than tuned after a
crash — nothing today relates the two, and the next board to add an entry finds
this the same way. Worth its own issue.

## Where to look## Where to look

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


## Mechanism addendum (2026-08-14) — two claims in the fix comment, corrected

The fix (`80442f438`, MAIN_STACK 16384 -> 131072 + `CONFIG_HW_STACK_PROTECTION`)
is right and is what closed this. Two statements in the board conf's comment
were not, and both were separately measured while arriving at the same root
cause. The conf now carries the corrected text; recorded here too because this
is the file the next person reads.

**1. The 88192-byte `NROS_EXECUTOR_SIZE` storage is not on the main stack.**
The C/C++ entry template declares it `static ::nros::Node __nros_node;` —
`.bss`, not a stack local. Cross-check that settles it independently: 65536 was
tried and passed all three cells 3/3, which is impossible if an 88 KB object
were on that stack. So "at the old 16384 that is a 5.4x overflow" is not the
arithmetic, and **shrinking `MAX_CBS` / `ARENA_SIZE` does not shrink what
`CONFIG_MAIN_STACK_SIZE` has to cover.**

What does overflow is the executor CONSTRUCTION path: ~13.4 KB already consumed
by the time `nros_cpp_init` is entered (`sp = 0x20075e88` against a
`z_main_stack` base of `0x20075320` — 2920 bytes left), then a ~9.3 KB temporary
cleared inside `Executor::assemble`. Caught with a ranged watchpoint over the
idle stack:

```
#0  __aeabi_memclr4          r0 = 0x20072e7c   r1 = 0x200752dc
#2  Executor::assemble       spin.rs:1318
#3  Executor::from_session_in spin.rs:1436
#4  Executor::open_in        spin.rs:176
#5  nros_cpp_init            nros-cpp/src/lib.rs:672
```

`0x200752dc` is inside `z_idle_stacks` (`0x20075220..0x20075320`).

**2. "Rust on this same board passes" is refuted**, which is the same
language-split premise this issue was originally filed on and the CORRECTION
section above already records. Measured on the Rust leaf against its own baked
port: **2 runs with a router reachable -> 2 faults; 2 runs without -> 0 faults.**
All three languages fault at 16384. The split is the ROUTER, because
`Executor::assemble` only runs once the session opens — nothing about the router
is special, it is just what makes the call chain deep enough.

**Why the mechanism matters even though the number is now generous.** 131072 is
comfortably clear either way. But a comment that attributes the overflow to a
sizing constant invites someone to reduce that constant and then reduce this
knob to match, which would silently reintroduce the fault — and the fault's
signature is an all-zero register dump against a thread that did not overflow.

### Confirming the PendSV mechanism

QEMU's `-d int` shows the switch to idle and the fault adjacent, with nothing in
between:

```
...taking pending nonsecure exception 14      <- PendSV
Exception return: magic PC fffffffd previous exception 14
...successful exception return
Taking exception 18 [v7M INVSTATE UsageFault]
```

`xPSR = 0` means the T bit is clear, which IS `INVSTATE` (`CFSR = 0x00020000`).
The all-zero registers are not a NULL call — they are the CPU faithfully
loading a context of zeros from a wiped idle stack.

### A dead end worth recording

A watchpoint on `z_main_stack - 4` never fires, which looks like conclusive
proof that main did NOT overflow. It is not: a large frame allocation drops `sp`
by ~11 KB in a single subtraction and writes nothing at the words it skips. Only
a watchpoint over the whole destination region catches the writer. This cost
several hours and briefly "refuted" the correct answer.
