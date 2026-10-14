---
id: 1911
title: "`sizes-header-mirrors` called a mirror stale when its build dir had simply not been rebuilt since a sibling rewrote the shared source, and stopped both scheduled tiers before the build"
status: resolved
type: bug
severity: high
area: [ci, build, testing]
related: [1858, 1783, 0268, 0805, 1832, 1905, phase-466, phase-489]
found: 2026-10-14
resolved: 2026-10-14
---

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)** — a scheduled lane that cannot reach its cells.

## Measured

`run-matrix` run 38637410239 was dispatched on `main` at `5ab72ab026`, right
after #2205 (issue 1905) and #2211 (phase-436 A2b/A2c) merged. Both tiers
stopped in `just build`, at its head, in `check fast`, on this gate:

```
STALE MIRROR: …/nros-fixture-tree/packages/testing/nros-tests/fixtures/image_config_ws/build/nros-metadata/metadata-probe-cmake/build/nano_ros/packages/api/nros-c/include/nros/nros_config_generated.h
         vs: …/metadata-probe-cmake/build/cargo/nros-fixture-tree_15c46/nros-c-generated/nros/nros_config_generated.h
             source: #define NROS_EXECUTOR_VALUE_SIZE 2256
             mirror: #define NROS_EXECUTOR_VALUE_SIZE 2248
3 stale sizes-header mirror(s)
```

The 8 B is #2211's two executor fields. The source is the new header. The
mirror is the old one.

## Cause

The mirror was not stale in the 0268 sense. Its build dir had not been built
since the source moved:

- **Shared source.** A probe's `cargo/` dir is shared between build dirs with
  equal configuration (issues 0805, 1858). After #2211, some other build dir
  sharing it rebuilt it, so the shared header changed.
- **Unrebuilt mirror.** `image_config_ws`'s probe build dir was not built in
  this run. Its mirror still held the old content.
- **It would fix itself.** Since issue 1783 the mirror command re-runs on
  every build of its dir, so the next build of `image_config_ws` copies the
  new header.
- **Gate timing.** The gate runs in `check fast` at the head of
  `just build tier1`/`tier2`, before anything is rebuilt. Since phase-489 W6
  the self-hosted lanes build in a persistent tree.
- **Consequence.** Every change to an executor or entity size made the next
  scheduled run of both tiers stop before the build.

The gate's "missing edge" message pointed at issue 0268's defect (a build
that ran and did not refresh its mirror). It had no way to tell that apart
from a dir that had not been built since the source moved: the copy is
copy-if-different, so the mirror's mtime says when its content last changed,
not when the command last ran.

## Fix

- `scripts/build/mirror-generated-header.sh` records each run for each dest in
  a marker file under `<build>/CMakeFiles/nros-mirror-ran/`.
  `--marker <build> <dest>` names that file, so its path is spelled once.
- `scripts/check-sizes-header-mirrors.py` counts a differing pair as drift
  only when the recorded run is at least as new as the source. That means the
  build ran its copy and the copy still differs, which is the 0268 shape and
  still fails.
- A pair whose source is newer than its last recorded run is "pending its
  build dir's next build". It is reported in the summary and not compared,
  like the existing "source gone" case. `--fix` still re-mirrors every
  differing pair.
- Both self-tests cover it. The mirror script's self-test checks that a run
  writes its marker. The gate's synthetic trees, for both the ninja and the
  Makefile generator, check that:
  - a newer source is pending;
  - the same pair after a recorded run is drift.

**Note.** A build dir built before this change has no marker, so a differing
pair there reads as pending until its next build, not as drift. Its next
build writes the marker.

**Acceptance.** A scheduled `run-matrix` run after this merges gets past
`check fast` at the head of `just build` in both tiers.
