---
id: 2840
title: "A tier-1 fixture build needs more than 127 GB on main, so it does not fit a 150 GB gate host"
status: open
type: limitation
severity: medium
area: [ci, fixtures, disk]
related: [issue-2702, issue-2838]
found: 2026-11-22
---

**Home:** [phase-489](../roadmap/phase-489-ci-build-speedup.md), the CI build
cost, alongside its runner-side items.

## What happened

Measuring issue 2702's per-image configures needed a full
`just build-test-fixtures tier1`, so it ran on newslab-243 under the host
lock, stopped when free space reached 50 GB. Both trees hit that floor in
the linux stage, before the lane finished:

| tree | start free | stopped after | peak use |
|---|---|---|---|
| main (`56f71a03ec`) | 174 GB | 2,160 s | ≥ 127 GB |
| issue 2702's branch, with its cargo prune | 117 GB | 1,530 s | ≥ 68 GB |

Where the disk sat when each run stopped, in GB:

| tree | main | branch |
|---|---|---|
| `build/zephyr-workspace-builds` | 31.8 | 31.9 |
| `build/sizes-probe` | 17.9 (109 dirs) | 24.8 (151 dirs) |
| `build/corrosion-cargo` | 7.7 | 7.7 |
| `build/metadata-probe` | 2.0 | 4.2 |
| `build/cargo-fixtures` | 2.3 | 2.3 |
| workspace cmake configures (`examples/**/build/<coordinate>`) | 31.0 (61 posix) | 10.8 (120 posix) |

So the lane does not fit a 150 GB host on main, independent of issue 2702,
which makes it ~13 GB lighter at the same point. A gate host is sized for
`just ci gate`, which builds no fixtures, so the gate is not affected. Any host
or runner expected to build the tier-1 fixtures is.

## Candidate mitigations

Ranked by nros 142 (2026-11-22), not yet measured:

1. **Extend the lane's cargo prune** to the Corrosion trees
   (`build/corrosion-cargo`) and the probe trees (`build/sizes-probe`,
   `build/metadata-probe`): their cargo `deps/` and `incremental/` once their
   artifacts are out. Issue 2702's `nros_prune_cargo_intermediates` does this
   for a workspace configure: 1,329 MB to 241 MB a configure, with a
   byte-identical rebuild.
2. **Not the Zephyr west build dirs.** The staleness probes read them, and a
   tree that lost them could not recover (issue 0834's shape). They stay.
3. **Split the lane**, building, testing and pruning one family at a time, only
   if the prune leaves the peak above ~100 GB.

Re-keying the sizes probe on its size-relevant fields only (issue 2838) cuts
the probe tree at its source.

Acceptance: a tier-1 fixture build measured end to end, under the host lock,
with its peak below the gate host's free-space floor.

## Measured end to end, 2026-11-24

nros 142 ran `just build-test-fixtures lane=tier1` on a clean worktree at
main `fb2c34e379` (which has issue B22's census-prepass fix, #4739),
aarch64, host 142. It finished, rc 0, in 6,800 s (1 h 53 min). `df` was
sampled every 10 s and `du` every 60 s. An earlier attempt at `6a86f706fe`
died in the census prepass after 1,957 s at a lower bound of 14.0 GiB, and
is superseded.

**Peak: +177 GiB of worktree** (`du` 4.2 to 182.1 GiB). The filesystem delta
was 181.2 GiB, including a little other activity on the host. The peak is
the END state: the lane only grows and frees nothing during the run.
Sampled every 10 min, in GiB: 4, 10, 14, 17, 27, 68, 70, 121, 137, 160, 181.
The two large steps came at about 40-50 min and 60-72 min.

Where it sat at the end, in GiB:

| tree | GiB |
|---|---:|
| `build/zephyr-workspace-builds/3.7` (32 dirs, ~1.22 each) | 35.0 |
| `build/sizes-probe` (one `rustc-1.99.0` dir) | 34.6 |
| `build/cmake-fixtures` | 11.0 |
| `build/corrosion-cargo` (native 4.8, metadata-probe 2.9) | 7.8 |
| `build/compile-check-fixtures` | 7.1 |
| `build/cargo-fixtures` | 2.5 |
| `examples/workspaces` (rust 12.9, features 8.1, cpp 3.8, c 3.1) | 37.3 |
| `examples/linux` (rust 10.9) | 13.7 |
| `packages/testing/nros-tests` | 18.6 |
| `packages/cli/target` | 2.4 |

sccache grew 0.4 GiB (30.2 to 30.6), outside the worktree. The shared caches
were warm (cargo registry, sccache, the `~/.nros` store, the Zephyr
workspaces), so a cold host adds the store on top of the 177 GiB.

**Acceptance is not met.** 177 GiB is above a 150 GB gate host, and above the
127 GB lower bound measured on 11-22. Against this table, the mitigations
above in their ranked order:

- mitigation 1 reaches about 55 GiB: `build/sizes-probe` 34.6, the
  Corrosion trees 7.8, plus the workspace configures, which issue 2702's
  prune already reaches;
- mitigation 2 is fixed at 35 GiB: the west dirs stay;
- `packages/testing/nros-tests` at 18.6 GiB is a tree none of the three
  names.

The raw samples and logs are kept on host 142 in
`/home/aeon/repos/nano-ros/tmp/issue-2840/`: `2840-run2-samples.tsv` and
`2840-run2-fixtures.log` for this run, `2840-run1-*` for the incomplete one.

## The two trees no mitigation named, measured (2026-11-24)

Measured read-only by nros 118 on the persistent fixture tree of the
`newslab-118-server` runner (`nros-fixture-tree`, which tier 2 and the
pairwise nightly build into and the runner sweep evicts by age). That tree
holds more than one tier-1 build: it accumulates across runs and lanes. So
its totals are larger than the 142 run's. The per-key and per-leaf shapes
are what carry over.

### `build/sizes-probe`

53 GiB: 271 keys under `rustc-1.99.0` (47 GiB) and 24 under a retired
1.96-nightly slug. Per key: median 177 MB, minimum 130, maximum 490.

**Where a key's bytes are.** A median key (`449d91ed94bef5e5`, 177 MB):

| part | MB |
| --- | ---: |
| host half (`nros-relwithdebinfo/`: build scripts, proc-macros, their deps) | 131 |
| target half (`x86_64-unknown-linux-gnu/nros-relwithdebinfo/deps`) | 44 |

Across all keys: host halves 26.7 GiB, target halves 11.9 GiB. The host
half has the same crate set and hashes in 158 of the keys, but it is not
byte-identical between keys (`toml_edit`, `serde_core` and
`nros_platform_config` all differ by content). So hardlinking identical
files would not reclaim it.

**Debuginfo.** The probe builds under `nros-relwithdebinfo` (`debug = 1`,
`build-override.debug = 0`). The DWARF sections in that key's rlibs sum to
8.7 MB, about 5 % of the key. Building the probe without debuginfo saves
about 5 %, roughly 2.4 GiB of the runner's 47.

**Is it issue 2838's fan-out?** In part. That fix (`1ad0b61571`,
2026-11-23) is in both this tree's latest run and the 142 run
(`fb2c34e379`). Grouping the keys by their recorded inputs
(`nros-probe-key-inputs.txt`):

| keys | all 271 | the 66 from today's run (pin baseline `3be75ab40a`) |
| --- | ---: | ---: |
| distinct as keyed | 271 | 66 |
| ignoring the orchestration knobs below | 249 | 66 |
| also ignoring `NROS_SIZING_DESCRIPTOR` | 115 | 46 |

So within one run, the descriptor still splits 66 keys into what would
otherwise be 46: about 30 %. The other 46 are real differences, in target,
features, board, entity counts and pool knobs. Across runs, a second source
splits keys: about 28 orchestration variables that `knob_identity()` hashes
because they are not on `KNOBS_THAT_CANNOT_CHANGE_A_SIZE`. Among them:

- `NROS_FIXTURE_COORDS`, `NROS_FIXTURE_FAILED_ROWS` and
  `NROS_FIXTURE_BUILD_LEDGER` (they differ per lane: tier 2 against the
  pairwise nightly);
- `NROS_SUBMODULE_PIN_BASELINE` (it differs per base commit);
- `NROS_RUNNER_*`, `NROS_LANE_INCLUDED`, `NROS_CENSUS_PREPASS`,
  `NROS_STAGE_ENV`, `NROS_SCCACHE_REAL`, `NROS_OFFLINE`, `NROS_TEST_*`,
  `NROS_LEAF_LOCK_LEDGER` and `NROS_CHECKOUT_MARKER`.

These account for 22 of the 271 keys (about 3.8 GiB) in the persistent
tree. It is phase-353 W4's class again, arriving by new names. The list was
written from one 2026-08-15 measurement.

### `packages/testing/nros-tests`

51 GiB on the runner tree (18.6 GiB in the 142 tier-1 run). It is the
single-node test binaries and fixtures: 73 leaves under `bins/` and 54 under
`fixtures/`, each its own cargo root. Several have one target dir per
variant (`message-info-observer` has three, about 1 GiB each), because a
cargo target dir serves one root (issue 0616).

| part | GiB |
| --- | ---: |
| cargo target dirs (`target*/`) | 43.0 |
| cmake build dirs (`build*/`) | 7.9 |
| of the target dirs: `deps/` | 31.9 |
| final executables (outside `deps/` and `build/`) | 0.6 |

So about 1 % of it is what the cells run. It is not issue 2838's fan-out.
It is one cargo root per leaf and variant, each with its own copy of the
core chain.

`scripts/ci/release-fixture-intermediates.sh --dry-run` over
`packages/testing/nros-tests/{bins,fixtures}` reports **39.9 GiB
releasable** (33.7 bins, 6.2 fixtures), 78 % of the tree. Live-peer's host
shards already reach this tree since phase-591 W3 derived their pack roots
from the manifest. The tier-1 lane does not: its only prune is the
workspace-configure one (`nros_prune_cargo_intermediates`).

### Ranked mitigation for these two trees, and what became of it

Ranked by the measurement above, then ruled on 2026-11-25:

1. **Release `packages/testing/nros-tests`' intermediates in the fixture
   lane.** Estimated at 78 % of that tree. **Not done: it backfires.** Tier
   1's run half checks freshness with `cargo build` per cargo row
   (`just ci tier1 run`, then `check-fixtures-stale.sh`, then
   `scripts/test/rust-fixture-stale.sh`), and cargo rebuilds a crate whose
   build-script executable or dependency rlib is missing. Measured on a
   scratch crate: deleting either one recompiles the crate and relinks the
   binary. So every released row (98 nros-tests rows in the manifest)
   would rebuild in the preflight, costing the build time again and
   bringing the disk straight back. Live-peer releases the same tree
   safely only because that lane never runs this probe.
2. **Prune each sizes probe's host half once the probe has answered.**
   Estimated at 74 % of a key. **Not done: it backfires.** The probe has no
   answer cache. Every call runs `cargo build` in its key directory and
   reads the sizes from the rlib it just built
   (`nros-sizes-build::find_dep_rlib_isolated`), so a pruned host half (131 MB of
   build scripts and proc-macros) recompiles on every reuse of that key.
   It would become safe only with an answer cache whose key includes a
   fingerprint of the probed sources, because the probe key does not: a
   source edit changes the sizes and not the key, and today cargo's own
   fingerprint is what catches it.
3. **Exclude the orchestration names from the probe key.** **Done**, as a
   harvest rather than a hand list
   (`scripts/gen-probe-orchestration-knobs.py`, checked by
   `just check probe-orchestration-knobs`). It stops cross-run fan-out on
   a persistent tree (22 of 271 keys here). In a single cold run it gains
   nothing.
4. **Build the probe without debuginfo.** About 5 % of the probe tree.
   Skipped as not worth a change.

Neither 1 nor 2 should be re-specified without the change it names first.
The next step is to re-measure a tier-1 build's peak with 3 in, then
choose between the Corrosion prune, the split lane, and cutting the
nros-tests duplication at its source. That last one is not "share target
dirs": a cargo `--target-dir` serves exactly one workspace root (issue
0616), so the 127 leaves would have to collapse into fewer cargo roots.

## Cutting the nros-tests duplication at its source, costed (2026-11-25)

Costed on paper by nros 118, from the manifests and a read-only pass over the
118 runner's fixture tree; nothing was built.

**What duplicates.** `packages/testing/nros-tests` holds 110 cargo manifests,
79 of them their own `[workspace]` roots. Of its 98 manifest rows, 59 are
`workspace_fixture` rows that `nros build` builds (38 of them linux), 17 are
plain cargo rows and the rest cmake or west. A workspace row already shares
by build class within its leaf (RFC-0100 D13): the class dir
`<target-dir>/nros-class/<key>/build` is cargo's `build.build-dir` and holds
intermediates only, while binaries are uplifted into the leaf's own
`<target-dir>/<profile>/`. The class key hashes content and no path, and it
repeats across leaves: 97 class dirs on the runner tree, 53 distinct keys,
one key in 9 leaves (`int32-sink`, `graph-probe`, the `action-*` bins, ...).
The copies beyond the first take **25.0 of 56.7 GB, 44 %**. At that ratio,
the 142 run's 18.6 GiB tree would lose about 8 GiB, and about 44 core-chain
compiles.

**(c1) Collapse the leaves into fewer cargo roots.** A cargo `--target-dir`
serves exactly one workspace root (issue 0616), so sharing across roots is
what this option avoids. Cost:

- package names collide across leaves (`talker_pkg` x6, `demo_entry` x5,
  `listener_pkg`, `ctrl_pkg`, `telem_pkg`, `alpha_pkg` x2), so they must be
  renamed;
- 44 leaves have path dependencies into their own `generated/`, and one root
  cannot hold two `std_msgs` path packages (a lockfile collision), so they
  need one shared generated tree;
- `nros build` and `nros configure` take a leaf to be a workspace, so they need a
  mode for several workspaces under one root;
- every row's `artifact_root` moves, and a row the resolvers cannot
  attribute fails closed in every lane (issue 0828), so the manifest columns
  and the resolvers move with it.

Several days and an RFC change, with lane-wide risk.

**(c2) Keep the roots; share only the class build-dir across leaves.** The
fixture lane names a lane-wide class root, so every leaf's image of one class
uses `build/nros-class/<key>/build`, while each leaf keeps its own
`--target-dir`. Artifacts do not move, so no resolver changes, and leaves
whose binaries share a name do not overwrite each other. Cost: about a day
for the `nros build` option and the lane wiring, plus three things that
must come first:

- **The premise, proven.** Issue 0616 still applies: one build dir is safe
  only if every root reaches each shared crate by the same path spelling,
  or two units of one crate land beside each other (with nros-platform's
  `#[global_allocator]` in both). The nros crates are external path
  dependencies from every leaf, so the spelling should agree; that is to be
  shown, not assumed. A two-leaf build of one class must show each nros unit
  once in the shared `deps/`, and the pair must include two leaves whose
  local packages share a name (`talker_pkg`), since rustc picks a transitive
  dependency by its SVH there and 0616 is where "should be unambiguous" last
  failed.
- **A real premise gate.** `check-path-env-fingerprints`'
  `SHARED_TARGET_PREMISE` checks that the sharing code exists, not that the
  spelling holds. The gate must check the spelling.
- **Wall time beside disk.** Cargo locks a build dir, so the leaves of one
  class queue behind each other: a key shared by 9 leaves turns 9 parallel
  builds into a line. The proof measures wall time as well as disk, and if
  the queue shows, each class is pre-built once before the leaves fan out.

**Ruling (2026-11-25):** (c2), behind the two-leaf proof and a premise gate,
after the re-measure of the tier-1 peak with the probe-key fix in. (c1) only
if the proof fails.

## (c2) built and proven (2026-11-25)

Host 118, `main` at `5058187aec`. Three leaves of one build class
(`int32-sink`, `int32-source`, `rosout-talker`; the lane's own `nros build`
command per row) were built into one class build dir.

**The premise holds.** Every shared nros crate gets one unit identity in every
leaf: `nros_core` is `530686a2…` and `97c3f434…` (target and host) in each
leaf's own dir, and those are the units the shared dir holds. A path package
outside the cargo root is hashed by its absolute path, and every leaf reaches
the nros crates outside its root. Crates whose FEATURES differ inside one class
(`nros_log` with and without `platform-clock`, and the crates above it) get
separate units side by side, which is correct: the class key hashes the
environment, not the feature set. Same-name member packages (`talker_pkg` x5)
never collide, because a generated entry reaches them outside its root too.
The one root-relative unit is the ROOT package, and every lane root depends on
a package of its own leaf (a generated entry on its members, a single-package
root on its `generated/<pkg>_nros_derived`). The two `linux_entry` roots
measured distinct (`7c2039fd…`, `9823b5c3…`).

**Two inputs named the leaf, and both made sharing worthless.** A forced shared
build dir recompiled 128 of 129 units on a no-change rebuild:

- Issue 2322's `--remap-path-prefix=<workspace>=.` is a rustflag. Cargo keeps
  rustflags out of `-C metadata` (so every leaf wrote the same file names) and
  checks them in every fingerprint (so every unit read dirty).
- The workspace road writes `NROS_SIZING_DESCRIPTOR` and `NROS_BOARD_TOML` as
  `relative = true` rows, so their text carries the leaf
  (`…/<leaf>/build/posix-zenoh/../../…`) and every build script watching them
  re-ran: 8 of 9 units recompiled. phase-489 W6 had made the descriptor row
  absolute on the single-package road for the same reason; the board row and
  the workspace road were never covered.

**What landed (lane only).** `NROS_BUILD_CLASS_ROOT`, exported by
`workspace-fixtures-build.sh` as `$(nros_build_root)/fixture-classes`, holds
every class dir instead of each leaf's `--target-dir`. Artifacts stay in each
leaf's target dir, so no resolver moves. Under it:

- an in-repo workspace remaps the CHECKOUT root, and the flag joins the class
  key. Issue 2322's purpose is that a copied-out project builds byte-identical
  from any directory (ruling 2026-11-19); an in-repo leaf never could, since
  it reaches the nros crates by absolute path ("an in-tree workspace still
  reproduces only at the same path"). So nothing it promised is lost. A
  workspace outside the checkout, and every build without a class root
  (users), keep the per-workspace remap. No test greps a remapped path;
- every path-valued settings row is absolute (`absolute_rows_under_class_root`).
  Without a class root the rows stay relative, so a user's leaf survives a
  moved checkout.

`just check shared-class-premise` checks the premise over the 70 rust
`[[workspace_fixture]]` leaves: every path dependency that leaves its leaf is
spelled as its real path and lies in no other lane leaf, and no two roots can
write one root unit.

**Measured** (warm `sccache`, the lane's `nros-relwithdebinfo` profile):

| Build | Units compiled | Wall | Class dirs on disk |
| --- | --- | --- | --- |
| int32-sink, first of the class | 129 | 18.0 s | 532 MB |
| int32-source, next | 9 | 13.2 s | 598 MB |
| rosout-talker, next | 10 | 3.9 s | 655 MB |
| int32-sink again, no change | 0 | 0.5 s | 655 MB |
| int32-source again, no change | 0 | 0.5 s | 655 MB |
| the three in parallel, shared (cold class) | 129 + 10 + 9 | 33.4 s | 655 MB |
| the three in parallel, unshared (cold, today) | 129 + 129 + 127 | 20.5 s | 1.6 GB |

Disk for the class drops 59 %. **Wall rises 63 % when a class's leaves run in
parallel**: cargo locks a build dir for a whole invocation, so they queue
(3–5 `Blocking waiting` lines each). The lane builds rows of DIFFERENT classes
side by side, so the queue costs only within a class.

**The fan-out orders by class** (`workspace-fixture-units.py after`, in the
same PR, so the tier-1 re-measure sees it). One unit per class LEADS, and
every other unit with a row of that class is a make prerequisite away from
its leader: it starts once the leader has built the class's shared units, and
does not hold a pool slot while it would only queue on the lock. The leader is
the class's smallest unit, so the wait is one image build and never a
workspace's serial chain; a leader waits for nothing, so the edges have no
cycle. The class key itself is computed by `nros build`, so the fan-out orders
by what the record states (image id, env, cargo args, board). A wrong guess
costs parallelism, never correctness. On the linux lane: 95 cargo rows in 46
stand-in classes, and 40 of 190 units wait for a leader (the largest class,
`linux`, has 36 rows).

## The class root needs a sweep area (2026-11-27)

(c2)'s shared class root, `build/fixture-classes/<root>/<key>/`, was in no
`runner-sweep` area. Two runs after it landed it held 60.4 GiB on
newslab-118's runner (112 class keys, about 0.5 GiB each), and the manager
reports the same dir on 241. The sweep reported it as unbudgeted every run and
never evicted any of it, so the runner volume drained by it. Each key is now a
unit of the `fixtures` area, as `sizes-probe/<slug>/<key>` is, evicted
coldest-first under the same budget. The budget was sized before (c2) moved
cargo intermediates out of the leaf dirs, so the leaf build units it already
covers should shrink as the classes grow; re-read the tier-1/tier-2 sweep
lines after a run with this in. Test: `runner-sweep-eviction-order`'s
class-key case, mutation-checked against the script without the change.

## Next step (2026-11-27)

Re-measure the tier-1 fixture build's PEAK end to end, with (c2) in
(`35e36102ab`, the shared class build dir; `81330b960e`, one unit per class
first, both on main since 2026-11-26), on a host with the disk. Not 118
(no gates there), and not 241 (68 GB free). The manager slots it on 142 or
243 after the nros12 dispatch.

#4959 (the runner sweep evicting `fixture-classes`) does not change this
measurement. The sweep runs between CI jobs on a persistent runner, never
inside one local `just ci` build, so it keeps the class dirs from
accumulating on a runner but does not lower a single build's peak.

If the peak is still over about 100 GB, the next mitigations, in this doc's
order: prune the Corrosion and probe trees (`nros_prune_cargo_intermediates`,
issue 2702), then split the lane.

## Re-measure with #4819, and the probe's profile-mode twins (2026-11-25)

**#4819 measured: no change, as its own numbers predict.** nros 142 re-ran
`just build-test-fixtures lane=tier1` at `939ac840b2`, which includes
`ac6969502a`. It used a clean detached worktree and the same df/du sampler as
the 11-24 baseline. The worktree peak went from 182.1 GiB to 182.2 GiB; the
filesystem delta was +177.3 GiB, against +181.2. #4819 removes only the
ACROSS-run key splits: orchestration variables that differ per lane, base
commit or runner, 22 of 271 keys on the runner's persistent tree. A single
clean run in a fresh worktree has none of them. Its saving lands on trees
that build repeatedly, like the runner's fixture tree, and this measurement
cannot show it.

**What the probe tree is made of in one run.** On host 142 (aarch64),
`build/sizes-probe/rustc-1.99.0-*` held 200 keys and 34.6 GiB, all written
by this run, with a median of 176 MiB per key.

- The host half (build scripts and proc-macros) is 22.0 GiB (64 %), and the
  target half 8.9 GiB (26 %).
- The 200 keys give **63** distinct size answers. The answer is the 18
  `__NROS_SIZE_*` markers in `libnros.rlib`.
- **The descriptor's splits are real.** Without `NROS_SIZING_DESCRIPTOR` the
  keys fall into 65 groups, but within the biggest group 28 of 33 keys give
  different answers (`EXECUTOR_SIZE` 0x1e398 against 0x16ef0, for example).
  The descriptor sizes the executor's pools, so it stays in the key.
- **`profile-mode` splits nothing.** Without it the keys fall into 111
  groups. 89 keys carry it, each with exactly one plain twin. All 89 pairs
  give identical markers. Those keys hold 15.3 GiB, 44 % of the tree, and
  every one of them is a duplicate.

**Fix:** `nros-sizes-build` drops `profile-mode` from the probe's key and
from its nested build (`SIZE_NEUTRAL_FEATURES`). The list is backed by a
measurement that runs as a gate, `just check probe-size-neutral-features`
(build tier). It builds `nros` with and without each listed feature and
compares the markers through the probe's own reader. It covers the seven
base feature sets that held a twin, on the host triple, plus one thumbv7m
set. Negative control: listing `lifecycle-services`, `rosout`, `sim-time` or
`param-store` fails it. The gate takes 74 s and 1.4 GiB on host 118.
Expected effect on the 142 run: 200 to 111 keys, about 34.6 to 19.3 GiB.

**Considered and rejected, measured:**

- *Keying the descriptor by the fields its readers use* (nros, nros-node,
  nros-params, rmw-cffi, xrce-cffi, zenoh) would merge about 5 of 33 keys in
  a group: 63 answers against 65 groups. It is not worth a derivation that
  has to track six readers.
- *Keying only the `NROS_*` names a compile can read*, instead of every name
  except the harvested orchestration ones. It would drop host-local names
  like the gate runner's `NROS_PEER_CMD_FILE`, which is in no tracked file.
  That saves nothing: such a name is constant per host and probe trees are
  per host, so it splits no key. It also flips the failure mode: a read the
  harvest misses (a vendored `build.rs`, a computed name) becomes a stale
  answer instead of an extra directory.

The same sweep showed that a few other features (`param-services`,
`metadata-mode`, the `nros-rmw/*-dispatch` pair, `safety-e2e`) change no
marker on these sets either. None of them produced a twin in the measured
tree, so they stay keyed until a measurement shows they cost a directory.
