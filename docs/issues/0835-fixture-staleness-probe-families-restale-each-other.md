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

## Direction 1 answered: it is not a SHARED artifact, it is a DUPLICATED one

The hunt asked which output one family produces that the other's signature
hashes. The answer is neither — the two families never share the directory at
all. They each get their own, because the shared-cargo key hashes a field that
does not affect what the directory holds.

`build/corrosion-cargo/threadx-riscv64/` holds FOUR groups where the
configuration space has two:

    036d16e80e45  platform=threadx          rmw=zenoh       board=riscv64-qemu  ...
    89bb1118ba8f  platform=threadx_riscv64  rmw=zenoh       board=riscv64-qemu  ...
    591f35a52a72  platform=threadx          rmw=cyclonedds  board=riscv64-qemu  ...
    4ec5af25ffb6  platform=threadx_riscv64  rmw=cyclonedds  board=riscv64-qemu  ...

Same board, target, profile and rmw; two spellings of one platform. Both are
populated with real builds — 2.8 GB / 12 `libnros_c*.a` against 595 MB / 14 for
the zenoh pair alone.

And the split is exactly by family:

    036d16e80e45 (platform=threadx)            6 leaves, all examples/.../rust/*
    89bb1118ba8f (platform=threadx_riscv64)   13 leaves, all c/ and cpp/

Six rust leaves x two rmw build dirs = the same **twelve** rows this issue
already identified as the ones the probe could not partition.

## Where the second spelling comes from

`examples/qemu-riscv64-threadx/rust/*/CMakeLists.txt:11`:

    set(NANO_ROS_PLATFORM threadx)

A plain `set()` creates a NORMAL variable that shadows the cache entry
`-DNANO_ROS_PLATFORM=threadx_riscv64` wrote, so everything downstream in that
leaf — including `packages/api/nros-c/CMakeLists.txt`'s
`nros_share_corrosion_cargo_dir(KEY "platform=${NANO_ROS_PLATFORM}" ...)` — sees
`threadx`. The C/C++ leaves set nothing and take the `-D` value. Both spellings
end up in one leaf's own state: its `CMakeCache.txt` says `threadx_riscv64`
while its `cargo` symlink points at the `platform=threadx` group, both written in
the same second.

## Why the obvious fix is wrong

Deleting the hardcode is not available: FIVE sites key on the exact string,
including the ThreadX carrier arm in `NanoRosNodeRegister.cmake:619` and
`nros-rmw-zenoh-staticlib/CMakeLists.txt:30`. The value is load-bearing for
cmake dispatch.

**But it is not load-bearing for cargo.** `NanoRosFeatureSet.cmake` resolves both
spellings to the same feature list on this board:

    threadx_riscv64  -> alloc platform-threadx
    threadx          -> if(_cross) alloc platform-threadx     # riscv64-qemu IS cross

So `NANO_ROS_PLATFORM` carries two meanings at once — a platform FAMILY for cmake
dispatch and a platform VARIANT for tier selection — and the cargo directory key
hashes the label instead of what the label resolves to.

## Recommended fix

Key the shared cargo directory on the **resolved cargo feature set**, not the
platform label. Features, profile and target are exactly what determine the
artifacts in that directory; the platform string is a cmake-dispatch detail that
happens to correlate. `nros_feature_set` already computes the list, and `caps=`
and `target=` are already in the key, so this is a substitution rather than a new
input.

Cost, stated plainly: every existing group hash changes, so the first build after
it lands is a full rebuild of the corrosion cargo dirs. That is a one-time price
for ending a permanent duplicate.

### The survey is done: ThreadX is the only platform that splits

    freertos          1 group   platform=freertos
    native            4 groups  posix x {zenoh, xrce, cyclonedds} + caps=safety   <- all legitimate
    threadx-linux     2 groups  threadx_linux x {zenoh, cyclonedds}               <- legitimate
    threadx-riscv64   4 groups  {threadx, threadx_riscv64} x {zenoh, cyclonedds}  <- HALF are duplicates
    nuttx, nuttx-riscv            a different key shape entirely (triple|profile|ffi)

Every other platform has exactly one spelling, and their multiple groups differ
by `rmw` or `caps` — real configuration differences. So the duplication is
confined to the six ThreadX rust leaves, and the blast radius of a re-key is
smaller than feared: no other pair would collapse, and none would split.

Not done here, and the reason is cost rather than doubt. Substituting the
resolved feature list for `platform=` changes EVERY group's hash, so the next
build after it lands rebuilds every corrosion cargo directory on every platform —
a one-time full rebuild that should be scheduled, not slipped into an unrelated
branch at the end of a session.

The two candidate fixes, for whoever takes it:

1. **Key on the resolved feature set** (recommended). Honest — features, profile
   and target are exactly what determine the directory's contents. One line at
   the call site in `packages/api/nros-c/CMakeLists.txt`. Re-keys everything.
2. **Normalise the platform label in the key only.** Narrow, no behaviour change
   anywhere else, and it fixes the one real case — but it encodes
   `threadx_riscv64 ~ threadx` as a fact about ThreadX rather than as the general
   rule, so the next platform that grows a variant repeats this issue.

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
