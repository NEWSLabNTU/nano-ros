---
id: 565
title: "SPLIT into #0569 (arm: session ConnectionFailed) and #0570 (riscv: main-task stack overflow) — filed as one bug on an inferred verdict"
status: resolved
resolved_in: issue-0565
type: bug
area: boards
related: [issue-0564, issue-0263, issue-0246, phase-281, phase-285]
---

## Symptom

`cargo nextest run -p nros-tests --test realtime_tiers_e2e`, with issue 0564's
timeout budget in place so the sweep actually completes:

```
realtime_tiers: 2 of 16 row(s) FAILED:
  nuttx-arm/rust:   [nuttx-arm rust] low-tier /telem never reached 5 deliveries —
      the low tier was not scheduled
      (phase-281 W3-nuttx: QemuArmVirt::run_tiers (std::thread per tier),
       the cell that completed the 12-cell Model-1 matrix)
  nuttx-riscv/rust: [nuttx-riscv rust] low-tier /telem never reached 5 deliveries —
      the low tier was not scheduled
      (phase-285 W6 / #165: QemuRvVirt::run_tiers)
```

14 of 16 rows pass, including every zephyr, freertos, threadx and native cell,
and including the NuttX **C/C++** cells. Only the two NuttX **Rust** rows fail,
and they fail identically on both arches.

## Why it was not seen before

It was hidden behind issue **0564**: the consolidated test ran on the default
60 s kill while needing 127–204 s, and rows are evaluated IN ORDER, so these
cells were never reached. The verdict said `TIMEOUT`, which says nothing about
the rows it never ran.

So this is not necessarily a NEW regression — it may have been failing for as
long as the budget has been wrong. Establishing when it started is part of the
work, and a bisect here must account for the truncation: any revision whose run
was killed before these rows reports TIMEOUT, not PASS, and must not be read as
good. (Issue 0268's lesson, in a different mechanism: when a bisect's first-bad
looks implausible, the test was tracking a confounder.)

## What the shape suggests

The high tier runs; the low tier does not. Both arches, Rust only, C/C++ fine on
the same boards — so it is unlikely to be the board's tier plumbing as such, and
more likely the Rust `run_tiers` path specifically: `QemuArmVirt::run_tiers`
(std::thread per tier, phase-281 W3-nuttx) and `QemuRvVirt::run_tiers`
(phase-285 W6 / #165).

Adjacent and RESOLVED, worth reading before starting: issue **0263** — "NuttX
Rust arm: spawned tiers get their stack but not their priority off the sporadic
path". Same family (spawned tiers on NuttX Rust), different failure: 0263 was a
wrong PRIORITY, this is no scheduling at all.

## Where to look

* `QemuArmVirt::run_tiers` / `QemuRvVirt::run_tiers` — does the second tier's
  thread get spawned at all, or spawned and never given a timer?
* the 100 ms tier specifically: the 10 ms `/ctrl` tier is observed fine, so
  whatever fails is not the tier mechanism in general.
* `nros_platform_clock_ns` landed the same week (RFC-0073 / phase-352) and
  phase-352 W6 retired the ms/us wrappers outright. A 100 ms period is exactly
  where a unit or resolution error would strand a timer while a 10 ms one keeps
  firing — worth ruling in or out FIRST, since it is cheap and the timing lines
  up. Issue **0532** (platform clock ABI unit and resolution) is the same area.

## Acceptance

* both NuttX Rust rows pass `realtime_tiers_e2e`, i.e. `16 of 16`;
* whatever the cause, the fix names which of the two arches' `run_tiers` paths
  was wrong and why it affected only the Rust lane.


## CORRECTION 2026-08-13 — the title above was wrong, and so was the shape

The verdict this issue was filed from ("the low tier was not scheduled") is the
test's INFERENCE from `/telem` never arriving, not an observation. It threw the
guest console away, so the two rows looked like one bug. They are not.

Made visible by teaching the verdict to print the guest's last 25 lines — the
same rule as issue 0445, and the reason that change is worth more than the
diagnosis it produced.

### nuttx-arm/rust — the tier runs; the SESSION dies

```
nros entry ready
nros: multi-tier run — 2 tier(s) over one session
nros: tier priority set tier=`low` prio=100
nros: core pin FAILED tier=`low` cpu=0 — kernel lacks CONFIG_SMP, tier runs unpinned
nros: RMW session open failed — ConnectionFailed
nros: Executor::open failed (Transport(ConnectionFailed)); multi-tier entry needs a live session — aborting
```

The low tier SPAWNED, got its priority, and reported its own core-pin fallback.
So `run_tiers` is doing its job and the scheduling hypothesis in the section
above is refuted for this row. What fails is transport: something opens an
Executor and cannot reach the router (`ConnectionFailed`), and the entry aborts.
The arm cell dials the slirp gateway `10.0.2.2` — that, the baked locator, and
whether this is a SECOND open (the guest rebooting into a port already held) are
where to look. The core-pin line is a red herring: it is a stated fallback, not
an error.

### nuttx-riscv/rust — a CRASH, not a missed deadline

The console is a NuttX assertion dump — `stack_dump:` pages followed by
`dump_tasks:`. The task table carries its own diagnosis:

```
dump_task: 3  3 100 RR Task    Running  … 0x800caea8  65208  58364  89.5%!  nsh_main
```

`nsh_main` at **89.5%** of a 65 208-byte stack, flagged `!`. Stack exhaustion on
the main task, not tier scheduling. Note the other pthread stacks are at 1.3%
and 0.5%, so the pressure is on the task that runs the entry, not on the spawned
tiers.

### What this means for the work

Two issues, not one, and neither is the title this was filed under. The
`run_tiers` / clock-unit leads in the sections above apply to NEITHER row and
should not be chased:

* arm — a transport/session failure; start from the locator and whether a prior
  instance still holds the port.
* riscv — a stack overflow; start from the entry task's stack budget
  (`CONFIG_PTHREAD_STACK_DEFAULT` / the nsh main stack) against what the Rust
  executor + zenoh-pico call depth needs.

They share only the cell family and the symptom the test reported, which is
exactly how one wrong verdict merged two unrelated defects.


## SPLIT 2026-08-13 — this issue is the wrong unit of work

Filed as one bug because the verdict said one thing. The console says two, with
nothing in common but the cell family:

* **#0569** — `nuttx-arm/rust`: tiers start, `Executor::open` then fails
  `Transport(ConnectionFailed)` and the entry aborts. A transport/session
  problem.
* **#0570** — `nuttx-riscv/rust`: `nsh_main` reaches 89.5% of its stack and the
  guest takes an assertion dump. A stack-budget problem.

Closed as SPLIT rather than fixed: nothing here was repaired, and leaving a
merged issue open would keep pointing the next reader at leads (the `run_tiers`
path, a clock unit/resolution error) that the console refutes for both rows.

The lasting change from this issue is `d97c9c606` — the verdict now prints the
guest's last 25 lines, which is what separated the two. Kept for the record
because the same mistake is cheap to repeat: an assertion that INFERS a cause
("the low tier was not scheduled") and discards the evidence will merge every
failure that shares its symptom.
