---
id: 459
title: "The Zephyr C++ realtime entry produces nothing after boot — reported as a missing EDF marker, actually a hang"
status: resolved
type: bug
severity: medium
area: zephyr, cpp
related: [issue-0422, issue-0445, phase-296]
---

## Symptom

`sched_dims_applied_e2e` fails one cell of twelve:

```
sched_dims: 1 of 12 cell(s) FAILED:
  EdfDeadline/zephyr/cpp: assertion `left == right` failed:
  [zephyr cpp EdfDeadline] expected exactly 1 `nros: EDF deadline set tier=`
  (the single declaring tier), saw 0.
```

Deterministic — 2 runs, 2 failures.

## What it actually is

The assertion counts one marker, so it reports a missing marker. The image
emits **nothing at all** after the Zephyr banner.

Run each realtime workspace image against the router baked into it (the baked
locator is the second `tcp/…` string in the binary; the harness supplies it, so
running the image bare makes BOTH lanes look silent and is not a valid
comparison):

```
# C — port 7591
zephyr-workspace/build-ws-c-realtime-entry-zenoh/zephyr/zephyr.exe
  → 1872 lines, "nros: EDF deadline set tier=`high` 10000us", "[nros] tier task entered"

# C++ — port 7691
zephyr-workspace/build-ws-cpp-realtime-entry-zenoh/zephyr/zephyr.exe
  → 4 lines total:
      WARNING: Using a test - not safe - entropy source
      *** Booting Zephyr OS build v3.7.0 ***
      (nothing for 20s)
      Stopped at 20.000s
```

So the C++ entry never reaches tier startup. `[nros] tier task entered` — which
precedes any deadline call — never prints either.

This is the issue-0445 absorption shape again: a narrow assertion at the end of
a chain names the last missing thing, and the reader takes "missing EDF marker"
for a scheduling problem. It is not a scheduling problem.

## Ruled out

* **Not the declaration.** All three workspaces
  (`realtime-{c,cpp,rust}/src/demo_bringup/system.toml`) declare
  `deadline_us = 10000`.
* **Not the kernel config.** All three `zephyr_entry/prj.conf` set
  `CONFIG_SCHED_DEADLINE=y`.
* **Not a stale image.** The C++ binary is NEWER than the passing C one
  (2026-07-31 vs 2026-07-24) and `strings` finds the
  `EDF deadline set tier=` format in it — the code is compiled in, the path is
  not taken.
* **Not the shared shim.** C and C++ both route through
  `packages/boards/nros-board-zephyr/c/zephyr_run_tiers.c`, and C works.

## Note on the third lane

`EdfDeadline/zephyr/rust` is reported as passing, but
`build-ws-rust-realtime-entry-zenoh/zephyr/zephyr.exe` does not exist on this
machine — the cell skips. Only the C lane actually proves the EDF path. Whether
the rust image is meant to be built by `just zephyr build-fixtures` and is
silently absent is worth checking alongside this.


## Reporting fixed, cause still open (2026-08-06)

The assertion no longer misattributes this. `nros_tests::output::
runtime_silence_note` classifies a log by whether the nano-ros runtime ever
spoke (every runtime line carries `nros`), and each shape in
`sched_dims_applied_e2e` now leads with it:

```
NO RUNTIME OUTPUT: 2 non-empty line(s), none from the nano-ros runtime.
  The image did not reach application code, so a missing marker below is NOT
  evidence about that marker — look between boot and the first `nros` line
  last line(s) seen:
    *** Booting Zephyr OS build v3.7.0 ***
[zephyr cpp EdfDeadline] expected exactly 1 `nros: EDF deadline set tier=` …
```

So the next reader is pointed between boot and the first runtime line, which is
where this actually lives, instead of at the scheduler.

Unit-tested both ways — a boot banner with nothing after it classifies as
silent, and a log containing any runtime line does NOT (otherwise the note would
swallow real missing-marker failures).

**The hang itself is unfixed and this issue stays open.** The "where to start"
section below is unchanged: between `main` and the first tier task in the C++
image — the generated C++ entry carrier, `nros_cpp_init`/session open, and
`ZephyrBoard::run_tiers`.

## Where to start

Between `main` and the first tier task in the C++ image: the generated C++ entry
carrier, `nros_cpp_init`/session open, and `ZephyrBoard::run_tiers`. The image
stops before any nros line, so suspect the entry carrier or the session open
rather than the tier loop. `NROS_RMW_TRACE_OPEN=1` and a gdb `run` (yama blocks
attach) on the native_sim binary are the usual tools.

## Does not reproduce (2026-08-07)

Rebuilt the zephyr fixtures and ran the C++ realtime entry against its baked
router (port 7691, the second `tcp/…` in the binary — the method this issue
prescribes):

```
$ timeout 18 zephyr-workspace/build-ws-cpp-realtime-entry-zenoh/zephyr/zephyr.exe
1644 lines
[nros] tier task entered
nros: EDF deadline set tier=`high` 10000us
```

1644 lines against the 4 reported, and BOTH markers — `[nros] tier task
entered`, which this issue notes never printed, and the EDF line the assertion
wanted. `sched_dims_applied_e2e` passes (12 cells, 0 failures).

The rebuild mattered for confidence, not for the outcome: the image already on
disk (built 2026-08-06 14:05) also emitted both markers when I ran it, so this
was not a museum binary either way.

**The fixing change is not identified, and I am not going to claim one.**
`73a3c4e44` (#458 — `nros_cpp_executor_open_over_session` never stamped `tag`,
so every C++ tier setup got `-3` and the tier was abandoned) matches the
mechanism exactly: "the C++ entry never reaches tier startup". But the working
image predates that commit by most of a day, so it cannot be the explanation for
THIS image. The likeliest remaining reading is that the run behind this report
used an image built before the 14:05 rebuild.

Also correcting the note at the end of the issue: the rust image is NOT absent.
It is `build-ws-rs-realtime-entry-zenoh` — `rs`, not `rust` — so the check that
looked for it was looking at the wrong path. All three realtime images exist.

Closing as not-reproducing rather than as fixed, since the difference matters if
it comes back.
