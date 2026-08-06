---
id: 459
title: "The Zephyr C++ realtime entry produces nothing after boot — reported as a missing EDF marker, actually a hang"
status: open
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

## Where to start

Between `main` and the first tier task in the C++ image: the generated C++ entry
carrier, `nros_cpp_init`/session open, and `ZephyrBoard::run_tiers`. The image
stops before any nros line, so suspect the entry carrier or the session open
rather than the tier loop. `NROS_RMW_TRACE_OPEN=1` and a gdb `run` (yama blocks
attach) on the native_sim binary are the usual tools.
