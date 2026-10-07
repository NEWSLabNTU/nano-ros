---
id: 1625
title: "`post-submit` dep-chain failed one cell on one commit and the cause is
  unrecoverable, because the FAIL diagnostic printed `thiserror` tree rows
  instead of cargo's error"
status: open
type: bug
area: ci
severity: low
found: 2026-10-02
related: [issue-0363, issue-1040]
---

## Measured

Run **36945033296** (`post-submit`, push, head `8cdeafcda`, 2026-10-02T00:15:04Z),
job **110645078601** `dep-chain (8 board×rmw cells)`, step 6 `just check dep-chain`:

```
dep-chain: 7 passed, 1 failed (of 8 cells)
  FAILED: qemu-armv7a-nuttx:zenoh
error: recipe `dep-chain` failed with exit code 1
```

The cell's own output is the whole of what was recorded about why:

```
  [ok] nros setup (toolchains + sources provisioned)
  [FAIL] cargo tree did not resolve (default features):
      │   │   │   │   │   │   │   ├── thiserror v2.0.21
      │   │   │   │   │   │   │   │   └── thiserror-impl v2.0.21 (proc-macro)
      │   │   │   │   │   │   ├── thiserror v2.0.21 (*)
```

**Those three lines are tree rows, not errors.** `scripts/ci/dep-chain-check.sh`
re-ran `cargo tree` on failure and filtered the merged stream with
`grep -iE 'error|failed'`; `thiserror` contains `error`, so the filter matched
the tree and the cause was discarded. Fixed in the same change as this filing —
the diagnostic now keeps **stderr**, which is where cargo writes diagnostics
while the tree goes to stdout. Measured both directions on a real failing leaf
and a real succeeding one: the error block is printed in full, and a successful
tree prints nothing.

## The red itself is a single occurrence

`post-submit` over the preceding twelve runs: **eleven consecutive successes**
(`d986c270a` 20:19 through `122202254` 23:21), this one failure on
`8cdeafcda`, and a **success on the very next commit** `2e093170d` (run
36945942254, 00:25). One cell of eight, one run.

The only property distinguishing that cell in that run: it is the only one whose
`nros setup` did fresh network clones, checking out three submodules it had not
had — `third-party/nuttx/{libc,nuttx,nuttx-apps}` — immediately before the cargo
probe. Every other cell reported `[already present (skip)]` for its sources.
That is a correlation and the mechanism is **not** established; it is written
here so a second occurrence can be compared against it rather than re-derived.

## What this is NOT

- **Not 1345.** Different lane and different gates; nothing here mentions
  `capability-conditionals` or `xrce-vendored-versions`, and the vendored trees
  this cell needs were provisioned in-run and reported so.
- **Not 1353.** No `No space left`, no `Free space left`, no truncation; the log
  is complete and the run ends with a recipe exit code.
- **Not 0363**, the stale-in-tree-`nros` cause whose symptom is exactly "cells
  whose printed cause is a cargo resolution error". The predicate that exists to
  front-run it ran and passed in the same job, one step earlier:
  `cli-fresh: OK — source-stamp: fresh (ca1431e41a134055)`.
- Not a lane with no verdict. `dep-chain` has its own job precisely so one red
  does not withdraw another lane's answer, and it reported.

## What would close this

A second occurrence, with the repaired diagnostic naming the cause — then fix
that. Or a measured mechanism for the fresh-clone correlation above, which would
make it a real ordering defect rather than a flake.

Closing it on the absence of a recurrence is also legitimate, but only once the
repaired diagnostic has ridden several `dep-chain` runs: the evidence that would
have attributed this one did not exist when it fired.

## Second occurrence — 2026-10-07, and the repaired diagnostic still named nothing

Run **37552262262** (`post-submit`, push, head `043cde8eb`, 2026-10-07T00:29:49Z),
job **112570268461** `dep-chain (8 board×rmw cells)`, step `just check dep-chain`:

```
  FAILED: qemu-armv7a-nuttx:zenoh
error: recipe `dep-chain` failed with exit code 1
```

Same cell as the first. The next `post-submit` (run 37554093307, head
`68c2ff9d0`, 00:50) was green on all 8 cells, again.

What the repaired diagnostic printed (cargo's `help:` text abridged):

```
  [FAIL] cargo tree did not resolve (default features):
      Updating crates.io index
      Updating git repository `https://github.com/NEWSLabNTU/ros-launch-manifest.git`
  warning: patch `libc v0.2.183 (/__w/nano-ros/nano-ros/third-party/nuttx/libc)` was not used in the crate graph
  help: Check that the patched package version and available features are compatible
      ...
      Locking 105 packages to latest Rust 1.96.0-nightly compatible versions
      Adding heapless v0.8.0 (available: v0.9.3)
```

**No `error:` line.** The repair kept stderr — right — but it did so by
**re-running** `cargo tree` and keeping `head -10`, and both halves lose the
cause:

- **The re-run is a different invocation.** A transient that failed the first
  `cargo tree` (a fetch of the crates.io index or the `ros-launch-manifest` git
  dep, both visible above) need not fail the second, so the diagnostic can be a
  SUCCESSFUL run's stderr printed under `[FAIL]`.
- **`head -10` keeps the preamble.** cargo writes `Updating` / `warning:` /
  `help:` / `Locking` / `Adding` BEFORE any error; those are the ten lines above.
  An error, had there been one, sat below the cut.

Fixed in the same change as this entry: `scripts/ci/dep-chain-check.sh` runs
`cargo tree` ONCE, captures that run's stderr and exit status
(`rc=0; err="$(…)" || rc=$?`, the issue-1249 spelling), and on failure prints
the exit code and the **last** 20 lines. Controls: a fake failing command
(stdout `TREE`, stderr `Updating` + `error: boom`, exit 101) prints `rc=101` and
both stderr lines, no tree; a fake succeeding one prints `rc=0` and an empty
capture.

**On the fresh-clone correlation:** it held again — this cell's `nros setup`
cloned `third-party/nuttx/{libc,nuttx,nuttx-apps}` `[provisioned]` immediately
before the probe, while the zenoh/mbedtls sources were `[already present]`. But
the `freertos` cell in the same run also cloned two fresh submodules
(`freertos/kernel`, `freertos/lwip`) and passed, so "a fresh clone before the
probe" is not sufficient by itself. What the nuttx cell has that freertos does
not is a `[patch]` onto a just-cloned tree (`third-party/nuttx/libc`, reported
above as "not used in the crate graph"). That is a second correlation, not a
mechanism.

**What would close this** is unchanged, with one input fixed: the next
occurrence now prints the failing run's own error.
