---
id: 835
title: "The cmake and rust fixture families re-stale each other, so
  `check-fixtures-stale` never reaches a fixed point and `just ci-matrix` fails
  ~190 tests on every run"
status: resolved
type: bug
area: testing
related: [issue-0828, issue-0196, issue-0466, phase-344, phase-340]
---

## Measured 2026-09-04 (phase-424) — the oscillation is GONE and now GATED; the budget

Phase-424 needs 0835's numbers as the budget its other seven issues are checked
against ("a fix that widens a watch set must show 0835 did not get worse, in the
same commit"), so this is the re-measurement. Everything below is measured
unless labelled otherwise.

### 1. There are FOUR probe families, not two, and they use TWO mechanisms

`scripts/check-fixtures-stale.sh` runs four probes over 371 manifest rows:

| family | rows | verdict is a function of |
| --- | --- | --- |
| cmake c / cpp | 61 + 59 | md5 of the cell build dir's top-level executables, BEFORE and AFTER `cmake --build` |
| cargo | 117 | md5 of the row's OWN binaries under its group `--target-dir`, before and after `cargo build` |
| workspace | 94 | `.inputsig`: a signature over a declared input set |
| compile-check | 40 | `.inputsig`: same shape |

The first two are **differential** — since 2fa1ed09f they have no watch set at
all. Their verdict is "did my artifact's bytes move", so no widening anywhere
can reach them, and family B may rebuild, evict, relink or re-date anything
family A reads without A firing. That is the structural reason the title's
oscillation is over, and it is why the budget below is about the OTHER 134 rows.

### 2. The differential probes reach a fixed point — measured against the real scripts

`tests/fixture-staleness-probe-tests.sh` (added by this commit, 2.2 s, on the
fast line as `just check fixture-staleness-probes`) drives the production probe
scripts against a two-file cmake cell whose always-dirty custom command
recompiles and relinks on every build — Corrosion's shape with no Rust in it —
and a one-binary cargo leaf in a phase-340-style group dir:

```
                                       pre-2fa1ed09f rule   today
cmake, unchanged tree, 3 runs               3/3 STALE        0/3
cargo, unit re-runs, bytes identical        3/3 STALE        0/3
cross-family: the cmake build dates the
  cargo row's source forward, 3 rounds      3/3 STALE        0/3
real source edit (both families)            reported         reported, once
```

The pre-fix rules are re-applied to the same trees inside the test as its
NEGATIVE CONTROL, so a probe silently broken into never reporting anything fails
it. Both mutations were run: reverting either script to its `2fa1ed09f^` version
fails the file (7 of 19 checks for the cmake half, 4 of 19 for the cargo half).

### 3. The `.inputsig` watch sets contain ZERO build output

Measured on a fully-built checkout (every one of these dirs has build trees; all
of them gitignored):

| enumeration | files | untracked-and-unignored |
| --- | --- | --- |
| 14 workspace fixture dirs (94 rows) | 568 tracked | **0** |
| 23 compile-check dirs (40 rows) | 243 tracked | **0** |
| dep-closure union over 38 built dirs | 792 distinct in-repo paths | 7 |

The 7 are 6 zenoh-pico submodule sources (tracked in the nested repo, kept
deliberately — `git check-ignore` refuses to answer inside a submodule) and
`packages/cli/third-party/play_launch/.git`, a gitfile whose content is a
constant. None is build output.

Both halves of a signature already use ONE policy for "is this build output?" —
git's ignore rules — which is what keeps this at zero.

### 4. The one genuinely shared input, and how much of its cost is inherent

`tool:nros` — the codegen fingerprint — is in all 134 `.inputsig` signatures, so
when it moves it re-stales every one of them. Measured on this host's cache:

    41 distinct `nros` binaries  ->  9 distinct codegen fingerprints

so 32 of 41 CLI rebuilds (78 %) re-staled nothing. The 9 that did are real: the
emitter's output changed. This is the phase-318 W1 fix working, and it is the
model for the budget below — key on what a tool EMITS, not on the tool.

It still over-approximates: a workspace row that runs no codegen hashes the
fingerprint anyway. That is a deliberate fail-safe and is the only known
spurious term.

### 5. THE BUDGET, stated for the other seven issues

* **Differential families (237 rows): spurious re-staling is 0, structurally.**
  There is no watch set. A fix elsewhere cannot make this worse. It CAN make it
  worse by changing the decision rule back to an activity signal — that is what
  the new gate refuses.
* **`.inputsig` families (134 rows): spurious re-staling is 0 today**, and stays
  0 while every path a signature hashes is one git does not ignore and no build
  writes. The cost of a proposed widening is arithmetic, not judgement:
  `(rows whose signature gains the path) x (how often the path moves)`.
  - a tracked source path: 0 until someone edits it — free;
  - a path some build WRITES: unbounded, and it is exactly how this issue
    happened. Nothing in the tree may hash one.
* **The shared-tool term is the one to watch.** A new input hashed into all 134
  rows costs 134 rebuilds every time it moves, so it must be keyed on what the
  thing EMITS, the way `codegen-fingerprint` is. Anything keyed on a binary hash
  re-stales 134 rows per `just setup-cli` — measured as the 41-vs-9 gap above.

Re-run the arithmetic with:

```sh
just check fixture-staleness-probes                    # the fixed point, 2.2 s
git ls-files --others --exclude-standard -- <sig-dir>  # must be empty
for f in .nros-cache/codegen-fingerprint/*; do cat "$f"; echo; done | sort -u | wc -l
```

### The two ways the differential families could still come back

Stated because "structurally impossible" is only true given two things, and both
are held elsewhere rather than by the probe:

1. **Artifact-name collisions inside one cargo group.** Cargo uplifts the final
   artifact to an UNHASHED name, so two rows in one group producing the same
   binary name would overwrite each other and each would see the other's bytes.
   That is phase-340's A1 precondition, gated by `check-fixture-groups` (fast
   line) at 0 collisions. If that gate is ever narrowed, this issue returns.
2. **A non-reproducible build.** The probe assumes a rebuild of unchanged inputs
   produces identical bytes. Verified for the synthetic leaf here and measured
   on the real fixtures by 2fa1ed09f. A toolchain or flag that embeds something
   varying (a timestamp, a temp path) would make the affected rows permanently
   STALE with nothing to fix — and it would look exactly like this issue.

### What is NOT fixed, and why it was left

The duplicated `threadx-riscv64` corrosion group (below, 2026-08-31) is still
real: the `set(NANO_ROS_PLATFORM threadx)` hardcode is present in all six
`examples/qemu-riscv64-threadx/rust/*/CMakeLists.txt`. But it is **wasted disk
and CPU, not staleness** — after 2fa1ed09f a duplicated group cannot make a
probe fire, because it cannot change a row's artifact bytes. It could not be
re-measured here: this host's `build/corrosion-cargo/` currently holds no
`threadx-riscv64` root at all (7 groups over 5 platforms, and every key text is
a real configuration difference — verified by reading the `.key` files).

Choosing between the two candidate fixes below re-keys EVERY corrosion cargo
directory on every platform, i.e. schedules a one-time full rebuild. That is the
phase owner's call, and deliberately not slipped in here.

## Problem (as filed — the oscillation below is FIXED; see the measurement above)

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


## RESIDUAL CLOSED 2026-09-05 — the cargo dir keys on the resolved feature set

The oscillation half was fixed and gated 2026-09-04. This closes the remainder:
the duplicated ThreadX corrosion groups, which were wasted disk rather than
staleness.

Done as this issue recommended — `packages/api/nros-c/CMakeLists.txt`'s key field
`platform=${NANO_ROS_PLATFORM}` becomes `features=<sorted resolved set>`.

**The duplication, re-measured from the `.key` files themselves before the
change:**

    036d16e80e45  platform=threadx         |rmw=zenoh     |board=riscv64-qemu|caps=|release|riscv64gc-…
    89bb1118ba8f  platform=threadx_riscv64 |rmw=zenoh     |board=riscv64-qemu|caps=|release|riscv64gc-…
    591f35a52a72  platform=threadx         |rmw=cyclonedds|board=riscv64-qemu|caps=|release|riscv64gc-…
    4ec5af25ffb6  platform=threadx_riscv64 |rmw=cyclonedds|board=riscv64-qemu|caps=|release|riscv64gc-…

Four groups differing in nothing but the label. The survey holds: freertos has
one group, native's four and threadx-linux's two differ by `rmw` or `caps`,
which are real.

**That the two spellings resolve identically is now measured, not read.** Driving
the real `nros_feature_set` for this board:

    threadx          ->  features=alloc,cffi-zenoh-cffi,platform-threadx,ros-humble
    threadx_riscv64  ->  features=alloc,cffi-zenoh-cffi,platform-threadx,ros-humble

so each pair collapses to one group: 4 -> 2 on this board.

**Dropping the label is safe because it is not load-bearing for CARGO, checked
rather than assumed.** `NANO_ROS_PLATFORM` reaches no cargo environment and no
build script reads it — the only references outside cmake are two tests
asserting that cmake sets it. It stays load-bearing for cmake dispatch, where
five sites key on the exact string; this changes what the DIRECTORY is keyed on,
not what the variable means.

The key is SORTED, because it must not depend on the order features happened to
be appended in.

**Cost, as this issue priced it:** every existing group hash changes, so the
first build after this lands rebuilds the corrosion cargo dirs once. The old
group directories are not removed by this change — they are inert and will be
reclaimed by whatever prunes build output, which is the disk-waste half of this
issue arriving one more time before it goes away.

**Not run:** no cross build was performed. The merge is established by driving
the real feature-set function and by the `.key` files above, not by observing two
leaves land in one directory.
