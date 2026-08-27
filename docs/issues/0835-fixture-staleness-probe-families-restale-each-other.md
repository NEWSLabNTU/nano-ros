---
id: 835
title: "The cmake and rust fixture families re-stale each other, so
  `check-fixtures-stale` never reaches a fixed point and `just ci-matrix` fails
  ~190 tests on every run"
status: open
type: bug
area: testing
related: [issue-0828, issue-0196, issue-0466, phase-344, phase-340]
---

## Problem

`scripts/check-fixtures-stale.sh` probes two families in order: cmake cells
first, then rust fixtures. Each family "self-heals" — it rebuilds what it finds
stale. **Rebuilding either family makes the other stale**, so a full run always
reports work and the tree is never in a state where both are fresh.

Measured on a tree where `just build-test-fixtures lane=all` had just completed
green (all nine legs OK):

```
run 1: 17 C/C++ cell(s) STALE and rebuilt · 23 rust fixture(s) STALE and rebuilt
run 2: 17 C/C++ cell(s) STALE and rebuilt · 23 rust fixture(s) STALE and rebuilt
run 3: 17 C/C++ cell(s) STALE and rebuilt · 23 rust fixture(s) STALE and rebuilt
```

Identical counts, identical membership. Not a treadmill converging — a fixed
oscillation.

The per-row probe DOES converge, which is what makes this hard to see:

```sh
bash scripts/test/rust-fixture-stale.sh "$row"   # prints the row  → stale
bash scripts/test/rust-fixture-stale.sh "$row"   # prints nothing  → fresh
```

And immediately after a full run — in which a cmake cell was rebuilt, and then
the rust family was rebuilt after it — that same cmake cell is stale again:

```sh
bash scripts/test/cmake-fixture-stale.sh "$c_row"
# examples/qemu-riscv64-threadx/c/talker/build-zenoh
```

So the rust rebuild writes something the cmake cells' input signature covers.
The two families share cargo outputs (the rmw staticlibs, the phase-340 shared
cargo group directory), and the signature on one side counts an artifact the
other side legitimately rewrites.

## Consequence

This is what keeps `just ci-matrix` red. The lane gate self-heals in-lane rows,
which stales out-of-lane rows that the lane never builds and — per issue 0828 —
cannot skip, and `test-all` then fails ~190 tests with

```
Workspace fixture <id> is stale: …/.nros-workspace-fixture.<id>.inputsig
```

Every one of those tests passes when run solo afterwards, because by then the
family it needed has been healed. Two consecutive `ci-matrix` runs on either
side of an unrelated change produced **the identical 92-test failure set** —
which is the signature of a build-state problem rather than a code one, and is
how it gets misattributed to whatever landed most recently.

## Fixed on the way: the probe's row selection

The rust probe selected rows by `--lang rust` while `is_cargo_row` has been
BUILDER-keyed since phase-344 W2. The twelve `examples/qemu-riscv64-threadx/rust/*`
rows (six zenoh, six cyclonedds) are `lang = "rust"` with `builder = "cmake"`,
so they were handed to `cargo build`, which cannot build a threadx cmake leaf:

```
ERROR: 12 rust fixture(s) could NOT be built by the staleness probe
  error: could not compile `nros-rmw-zenoh-staticlib` (lib)
```

A row the probe cannot build is never fresh, so those twelve were stale on every
run forever. They were also in the cmake list, where they self-healed correctly,
so each was reported twice under two labels — and the ERROR block named them
`build-zenoh` with no leaf path, which read as unattributable rather than as a
partition bug. Now `--builder cargo`, matching `is_cargo_row`. That removed the
ERROR block; the oscillation above is what remains.

## Directions

1. **Find the shared artifact.** The candidates are the rmw staticlibs and the
   phase-340 shared cargo `--target-dir` group: an output one family produces
   and the other's `.inputsig` hashes. Whichever it is, one side is treating a
   BUILD OUTPUT as an INPUT.
2. **Probe order must not matter.** A gate whose result depends on which family
   it looked at first is not measuring freshness. Once (1) is known, either
   exclude the shared output from the signature or make both families derive it
   from the same producer.
3. **Until then, `check-fixtures-stale` cannot be trusted to mean "the tree is
   stale"** — it means "someone rebuilt something". Its self-heal makes each run
   look successful, which is why this survived: the WARNING path reads as the
   gate working.

## Sweep

```sh
grep -rn 'inputsig' scripts/test/*.sh scripts/build/*.sh | head
grep -rn 'nros_fixture_target_dir_flag\|cargo-fixtures' scripts/
```
