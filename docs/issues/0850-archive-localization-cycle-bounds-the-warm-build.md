---
id: 850
title: "A copy/localize cycle re-processes libnros_cpp.a every build, and now bounds the warm wall at 85% disk-wait"
status: open
type: performance
area: build
related: [issue-0805, issue-0648, phase-371]
---

## What bounds the warm build now

Measured on `threadx_riscv64` after issue 0805's work took the warm rebuild from
459 s to 227 s. Three instruments, in the order that made each next question
answerable.

**Per-leaf spans** (`scripts/build/sample-build-leaves.sh`), 240 s wall, 83% of
process-samples leaf-attributed:

```
prologue   0- 71s   four C/C++ group calls, SEQUENTIAL: 7 leaves/25s,
                    6/21s, 2/8s, 2/3s — each using 2-7 of 32 cores
body      73-236s   all 12 rust leaves start together, 149-163s each
```

So the wall is `73 + 163`. The body is not a queue — every leaf starts at t+73.

**Occupancy** (`sample-build-lineage.sh`):

| | |
| --- | --- |
| alive | 61.5 |
| **runnable** | **0.14** |
| **disk-wait** | **7.30** |
| loadavg runnable | 2.7 |

52x more processes blocked on disk than running. It is NOT CPU-bound — a
hypothesis this issue's author held and this measurement refuted.

**Blockers** (`sample-build-wchan.sh`), leaf state 85% D / 14% S / 1% R:

```
1075  llvm-ar       rq_qos_wait     block-layer writeback throttling
 137  llvm-objcopy  rq_qos_wait
  29  cargo         locks_lock_inode_wait
```

`llvm-ar` by an order of magnitude. Not locks — 0648 already established that
block COUNTS are not costs, and these are occupancy samples, not events.

## The cycle

`cmake/strip-compiler-builtins.sh` runs from the link wrapper, once per archive
per link. Issue 0805 gave it a size+mtime stamp so an unchanged archive is
skipped, which took the count from 190 to 17 per warm build. The residual 17 are
all `libnros_cpp.a`, and they are structural rather than a stamp failure:

1. Corrosion copies the **unlocalized** archive from the shared cargo dir into
   each leaf (`copy_if_different`).
2. The link wrapper localizes the leaf's copy — modifying it.
3. Next build, `copy_if_different` compares the shared (unlocalized) source
   against the leaf's (localized) copy, sees a difference, and copies again.
4. The stamp is now stale, so the archive is re-processed. Go to 2.

Each processing extracts every member and makes six `llvm-objcopy` passes —
4.3 s on a 1.6 MB archive when alone. Seventeen of those used to be spread
across a serial tail and were invisible; issue 0805 made the rust leaves
concurrent, so they now run at once and saturate the disk queue. **The fix that
removed the serial bound exposed this one** — which is the expected shape, not a
regression.

## Candidate fixes, and why none is obviously safe

* **Localize once, at the source.** If the archive in the shared cargo dir were
  already localized, every leaf's copy would match and the cycle breaks. But that
  file is cargo's output; modifying it in place risks cargo seeing it as dirty
  and rebuilding — the rebuild-forever class this campaign has already hit twice
  (issues 0491, 0805).
* **Stamp on content rather than mtime.** Does not help: the copy restores the
  pre-localization content, so a content stamp mismatches for the same reason.
* **Have the link consume the shared archive directly**, so there is no per-leaf
  copy to localize. Closest to correct; needs the link line examined.

## Also worth fixing, separately and cheaply

The prologue's four `fixtures-build.sh` group calls are SEQUENTIAL and each uses
2-7 of 32 cores (25 s, 21 s, 8 s, 3 s). Running them concurrently is a different,
smaller win — bounded by ~40 s of the 240 s — and does not interact with the
above.

## Acceptance

* `llvm-ar` stops appearing as the dominant leaf blocker in a wchan sample of a
  warm `threadx_riscv64` build.
* The count of archive re-processings per warm build goes to 0, verified by
  counting `Localized … mem symbols` lines, not by reading the script.
* Whichever fix lands, prove the produced binaries are byte-identical to the
  current ones — this whole area has repeatedly failed as a wrong artifact
  rather than a build error.
