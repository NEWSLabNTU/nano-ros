---
id: 2378
title: "Two fixture rows of one leaf take its census at once and clobber the shared `census-host` build dir, so tier 2 stops at configure"
status: open
type: bug
severity: high
area: [build, zephyr, ci, census]
related: [2365, 1252, 2221, phase-530]
found: 2026-11-09
---

**Home: [phase-530](../roadmap/phase-530-nros-configure-single-pass.md)** -- W2 made
the census the default entity source for a leaf that states no `entities`.

## Measured

Tier-2 run 42605683876 (`workflow_dispatch`, main `a9dc35a5e1`, 241 runner)
was the first run since about 2026-11-07 to get past issue 2365. It stopped
in `just build tier2`, in the Zephyr stage. The two failing rows are both
`examples/zephyr/c/talker`:

- `build-c-talker-xrce` (native_sim, XRCE)
- `build-nucleo-f767zi-c-talker-serial`

Neither one fails at its image link. Both fail at CONFIGURE, in the census
step: `NanoRosLeafCensus.cmake:79` runs `nros dev image entity-census take
--leaf <leaf>`, which configures and builds the leaf for the host in
`<leaf>/build/census-host` (`entity_census.rs`). The per-row logs are in the
run's `post-submit-junit` artifact, under `build/zephyr-fixtures/`:

```
-- Check for working C compiler: /usr/bin/cc - broken
  : && /usr/bin/cc   CMakeFiles/cmTC_dac42.dir/testCCompiler.c.o -o cmTC_dac42   && :
  /usr/bin/ld: cannot find CMakeFiles/cmTC_dac42.dir/testCCompiler.c.o: No such file or directory
```

and, for the nucleo row:

```
  '/home/runner/.nros/sdk/ninja/1.13.2/bin/ninja' '-C' '.../examples/zephyr/c/talker/build/census-host/CMakeFiles/CMakeTmp' '-t' 'recompact'
  ninja: error: loading 'build.ninja': No such file or directory
```

On the 241 runner itself (read-only), `census-host/CMakeFiles/CMakeError.log`
(11:25Z) shows try_compile files disappearing mid-configure in three ways:
`ninja: error: unknown target 'cmTC_924fe'`, `as: Fatal error: can't create
CMakeFiles/cmTC_d0d93.dir/CMakeCCompilerABI.c.o`, and the `ld: cannot find`
above. The compiler and its implicit link dirs are all valid in the current
image (gcc 11.4.0), so this is not a stale cache, and a later job on that
runner reconfigured the same dir cleanly.

The object was compiled and then deleted before the link. Each row's
configure deleted files the other had just written, in the same
`CMakeFiles/CMakeTmp`. The runner's host compiler is fine: the seven
workspace XRCE images on native_sim link with the same `/usr/bin/cc` in the
same run.

Reproduced on newslab-118 at main `f05c0c862b`, with the leaf's census
made stale (its `census-host` dir and `leaf.census.json` moved aside) and
three `nros dev image entity-census take --leaf examples/zephyr/c/talker`
started at once. One of three failed:

```
/usr/bin/ranlib: unable to copy file 'libtalker_lib.a'; reason: No such file or directory
Error: .../examples/zephyr/c/talker: the build of its host census build exited exit status: 1.
```

Run one at a time, the same leaf takes its census cleanly, and a filtered
fixture build with only one row of the leaf (`NROS_ZEPHYR_FIXTURE_FILTER=xrce`)
builds all 22 rows.

## Why

The census dir is per LEAF (`<leaf>/build/census-host`), and a fixture lane
builds several ROWS of one leaf in parallel: the Zephyr pool runs 24 at once.
Each row's configure runs the census hook, and `entity-census take` takes no
lock. When the census is fresh, every row reads it and nothing is written, so
the race stays hidden. When it is stale (the leaf's sources moved, or the tree
has never run it), every row rebuilds the same dir at the same time.

Main is not red on every run because the census is usually fresh in a fixture
tree that persists. It goes stale exactly when a change touches the leaf,
which is also when the run matters.

## Fix

`take --leaf` (`take_leaf_census_with` in `entity_census.rs`) holds an
exclusive `flock` on `<leaf>/build/census-host.lock` around the freshness
check AND the rebuild, using the CLI's one lock helper
(`file_lock::lock_exclusive_path`). A second taker waits, then finds the
census fresh and does nothing. The lock is its own file, not the configure
lock, because a configure that holds that lock may be the caller. The host build
cannot re-enter, since `NROS_LEAF_CENSUS_HOST` stops every hook.

Measured: the three-way repro above, run as three rounds of four concurrent
takes on a stale leaf, gives 12 of 12 passing. In each round one taker
rebuilds and three find the census fresh. Unit test
`two_takers_of_one_leaf_never_build_its_census_at_once` counts overlapping
host runs; with the lock removed it fails.

Sweep: `git grep -nE 'leaf\.join\("build"\)' -- packages/cli`. The other
per-leaf writers (`leaf_settings.rs`, `sizing_descriptor.rs`) write under
`build/<image_id>`, which is keyed by image, not shared across rows. The
census JSON (`build/nros/census/leaf.census.json`) is now written under the
lock. A workspace's census takes the workspace configure's lock already.

## What would close it

Concurrent takes on one stale leaf all succeed (the three-way repro above,
as a test), and a tier-2 run builds both `examples/zephyr/c/talker` rows.
