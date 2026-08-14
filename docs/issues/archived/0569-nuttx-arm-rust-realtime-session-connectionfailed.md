---
id: 569
title: "nuttx-arm Rust realtime entry: the tiers start, then `Executor::open` fails `Transport(ConnectionFailed)` and the entry aborts"
status: resolved
type: bug
area: boards
related: [issue-0565, issue-0570, issue-0246, phase-281]
---

## Symptom

`realtime_tiers_e2e`, row `nuttx-arm/rust`. The guest console (printed by the
verdict since issue 0565):

```
nros entry ready
nros: multi-tier run — 2 tier(s) over one session
nros: tier priority set tier=`low` prio=100
nros: core pin FAILED tier=`low` cpu=0 — kernel lacks CONFIG_SMP, tier runs unpinned
nros: RMW session open failed — ConnectionFailed
nros: Executor::open failed (Transport(ConnectionFailed)); multi-tier entry needs a
      live session — aborting.
```

The test then reports `low-tier /telem never reached 5 deliveries`, which is an
INFERENCE from the missing telemetry, not what went wrong.

## What the console rules OUT

* **Not a scheduling bug.** The low tier spawned and `tier priority set
  tier=\`low\` prio=100` printed, so `QemuArmVirt::run_tiers` did its job.
* **Not the core pin.** `core pin FAILED … kernel lacks CONFIG_SMP, tier runs
  unpinned` is a stated fallback, not an error — a red herring on this path.
* **Not a spawn failure.** The `#246` retry path prints
  `FAILED to spawn tier … after N attempts`; it did not.

## What it points AT

A session open that cannot reach the router. `run_tiers` opens the ONE session
on the boot tier and every spawned tier borrows it via `open_with_session`, so a
`Transport(ConnectionFailed)` from `Executor::open` after the entry banner is
already printed means an open happened LATER than the boot open — worth
establishing which one, because that is either a second open that should not
exist or a guest that rebooted into a port a previous instance still holds.

Where to look:

* the baked locator: the arm cell dials the slirp gateway `10.0.2.2:<port>`; the
  port is `nros_tests::alloc::port_of`'s `RealtimeTiers` number, the same formula
  the fixture bakers use, so a hand mismatch is unlikely but the BAKED value is
  worth reading out of the image;
* whether the guest restarted — the abort path calls `std::process::exit(1)` and
  NuttX may relaunch `nsh_main`, in which case the second boot's open races the
  first instance's socket;
* the router: the host `zenohd` for this row is started by the test on that port.

## Note

Sibling row `nuttx-riscv/rust` fails in the SAME test with a completely
different mechanism (a stack overflow — issue 0570). They were one issue (0565)
until the verdict started printing the console; do not assume a shared fix.

## Acceptance

* the `nuttx-arm/rust` row of `realtime_tiers_e2e` passes;
* the fix names why the failing open happened after the entry banner.


## RESOLVED 2026-08-14 — same root cause as #570, which the note above told you not to assume

#570 turned out not to be a stack overflow at all: the vendored NuttX `libc`
fork mirrors `pthread_attr_t` as 20 bytes while NuttX's (with
`CONFIG_SCHED_SPORADIC=y`, which BOTH boards set) is 56, so
`pthread_attr_init`/`pthread_attr_destroy` write 36 bytes past the end of the
object `std::sys::thread::unix::Thread::new` put on its own frame.

On arm that lands on `push {r4, r5, r6, r7, r9, lr}` — `attr` sits at `sp+0` of a
32-byte local area and the write runs 24 bytes into the pushed registers. Same
defect, different registers, different symptom: riscv returned to address 0,
arm corrupted live state and the session open failed.

Fixed by `__PTHREAD_ATTR_SIZE__` 5 -> 14 in the fork (#570). `realtime_tiers_e2e`
now passes **16 of 16 rows**, this one included, with no change to any transport
or session code.

The note above was right to demand evidence before assuming a shared fix, and
wrong about the answer — the two mechanisms it called "completely different"
were one write. What settled it was not the reasoning either way but the
`-d exec` trace naming the faulting instruction.

## Acceptance — met

* the `nuttx-arm/rust` row passes (16/16, 88.8 s);
* why the open failed after the banner: it did not fail on its own terms —
  `Thread::new` had already smashed the caller-saved registers by the time the
  session was opened.
