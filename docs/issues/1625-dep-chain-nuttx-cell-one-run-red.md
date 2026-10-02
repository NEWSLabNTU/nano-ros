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
