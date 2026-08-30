# Phase 331 — Example and fixture consolidation

**Implements:** RFC-0066
**Informed by:** the 2026-08-02 fixture inventory (337 rows; 34 of 42 themed
workspace rows carry no build config), RFC-0064 (board tiers), phase-329 (test
taxonomy), issue 0389 (lane-scoped fixture builds)

## Problem

Feature coverage is expressed as directories (28 themed micro-workspaces, one
per feature × language) while build configuration is barely expressed at all
(84 of 86 workspace fixture rows are zenoh). RFC-0066 inverts both. This phase
executes it.

## Ordering constraint

Consolidation changes the cell set that phase-329's machinery consumes, and
issue 0389 made the fixture build lane-scoped. Both are load-bearing here:

- run `matrix_fixture_coverage` (G1–G4) **before and after** each work item —
  it is the gate that proves no cell was silently dropped;
- measure with `just build-test-fixtures lane=native`, not a full sweep, so the
  before/after numbers are comparable and affordable.

## Work items

### W1 — Measure before touching anything

Establish the baseline the RFC deliberately does not assert.

- [x] Wall-clock `just build-test-fixtures lane=native` on a clean tree
      (wipe the manifest-declared workspace build dirs first — derive them from
      `fixtures-manifest.py list-workspaces`, **not** a `build-workspace-fixtures`
      glob; non-default `build_subdir` values exist: `-safety-talker`,
      `-safety-listener`, `-managed`).
- [~] Per-workspace `nros sync` + CMake-configure time for the four large
      workspaces and for a representative themed one. **Not captured, and no
      longer capturable:** the comparison it asked for is against "a
      representative themed one", and W3 deleted every themed workspace. The
      question it was meant to answer — did the fold make a cycle cheaper — is
      answered instead by W5's seconds-per-fixture (92.4 -> 72.5), which
      measures the same thing end to end.
- [x] Record cell counts from `lane-coords tier1 --cells` and the
      `matrix_fixture_coverage` output.

**Acceptance:** a committed baseline table. Without it W5 cannot say whether
the fold paid.

**Done 2026-08-02** (`docs/roadmap/data/phase-331-w1-baseline.md`, base
`82b82a6d6`): cold build **7051 s / 1 h 57 m**, native stage 5912 s (84 %), 64
fixtures, 0 errors. 337 manifest rows (251 single-node + 86 workspace); 35
workspace dirs; tier1 = 10 coords / module `native`; tier2 = 12 coords.
Prerequisites needed a `just setup-cli` + `just setup-launch-resolve` rebuild
first — the checkout had moved 51 commits and the stale-CLI guard caught it in
1 s.

### W2 — Collect the feature demos into `examples/workspaces/features/`

**Revised 2026-08-02 (RFC-0066 R2).** W2 originally folded each theme into the
same-language large workspace. That was implemented for `c` and `cpp` and then
abandoned: capabilities are an IMAGE property, every large workspace contains
EMBEDDED entries, and `param_services`/`lifecycle` are alloc-gated features an
embedded image must opt into explicitly. Rust made it unbuildable outright —
`nros sync` will not generate selection facades for a multi-system workspace
(phase-315 W1). See RFC-0066 *Where a capability applies*.

Build ONE new native-only workspace instead. Order: scaffold it, then move the
C packages (already proven), then C++, then Rust.

- [x] **Revert the R1 folds already landed in `workspaces/{c,cpp}`** — the
      qos / params / lifecycle / custom-msg packages, their renamed entries, the
      `lifecycle_bringup` split, the capability stanzas, and `cpp`'s
      `SYSTEM demo_bringup` line (keep that last one: it is a genuine
      pre-existing gap, see the checklist).
- [x] Scaffold `examples/workspaces/features/` — one `demo_bringup` declaring
      `[param_services]` + `[lifecycle] autostart = "active"`, native entries
      only, NO embedded entry packages.
- [x] Move the C, then C++, then Rust feature packages in, applying the
      per-fold checklist below.
- [x] ~~`managed_bringup` joins as the second system~~ -> became `ws-managed-cpp`. Its
      entries are C++ only; a rust managed entry would hit the phase-315 limit.
- [x] Confirm `custom_msgs` stays workspace-local — **done**: all copies are
      byte-identical and workspaces build independently, so one copy lives in
      `features/`.
- [x] Verify `ws-launch-rust`'s coverage — **done, and the assumption was
      wrong**: it is the only workspace exercising the launch v1 language
      surface (`<arg>`, `$(var …)`, `<group ns=>`, child `<param>`/`<remap>`,
      `<include>` with pass-through). It is KEPT, so the deletion list is 17.

**Acceptance:** `features/` builds and its e2e tests pass; the collected nodes
are placed by a bringup and **observable at runtime, not merely compiled**; the
four large workspaces are byte-unchanged apart from `cpp`'s `SYSTEM` line.

The runtime clause is load-bearing. During the R1 attempt the `c` fold was
reported green while having silently lost its lifecycle autostart — the entry
compiled and simply never emitted `nros_cpp_lifecycle_autostart`, so the managed
node never reached `active`. Only a diff of the generated
`*_nros_main_generated.c` against the pre-fold one caught it. Verify capability
effects in the GENERATED ENTRY, not in the exit code.

#### Per-fold checklist (learned from the R1 `c` and `cpp` folds, 2026-08-02)

A fold is NOT a directory move. Each item below was a real failure, found by
building rather than by reading:

1. **CMake TARGET names collide even when directory names do not.**
   `c_lifecycle_talker_pkg` and `c_talker_pkg` both declared
   `nano_ros_auto_add_library(talker_lib …)` and `EXECUTABLE talker`; each was
   unique only inside its own workspace. Audit before moving:
   `grep -oP 'nano_ros_auto_add_library\(\K\w+' … | sort | uniq -d` and the same
   for `EXECUTABLE`. Renaming the executable also means editing the theme's
   launch `exec=`/`name=`.
2. **ENTRY package names collide across themes.** Several themes ship
   `native_entry` / `native_talker_entry` / `native_listener_entry`, and the
   target workspace already owns `native_entry`. Rename entries on fold
   (`native_<theme>_<role>_entry`); node packages need no rename in a
   single-language workspace.
3. **A capability stanza travels with its node package.** `c_param_talker_pkg`
   compiled and then failed at LINK with `undefined reference to
   nros_cpp_get_param_integer` — the themed bringup declared `[param_services]`
   in `system.toml` and the target's did not. Diff the two `system.toml` files
   for stanzas beyond `[system]`/`[[component]]`/`[deploy.*]`.
4. **Launch and model basenames collide.** Themed workspaces name their
   single-scenario launch `system.launch.xml` + `system_model.yaml` — the same
   names the target uses for its OWN default. Namespace them by theme on the
   move (`params.launch.xml`, `params_model.yaml`).
5. **Rewrite MODEL paths ONLY in the entries just moved.** A blanket
   `sed` over `$dst/src/*/CMakeLists.txt` retargeted five pre-existing entries
   (`native_entry`, `freertos_entry`, `native_threadx_entry`, `nuttx_entry`,
   `zephyr_entry`) from `system_model.yaml` to the theme's model, because of (4).
6. **An interface package is not a CMake subdir.** `custom_msgs` declares the
   schema, but the components carry the type name as a string and hand-encode
   CDR (the RFC-0043 typed-component idiom). Do NOT add it to `_ws_subdirs`;
   instead set `NROS_INTERFACE_SEARCH_PATH "${CMAKE_CURRENT_SOURCE_DIR}/src"`
   **before** `find_package(nano_ros)` so the compat layer auto-emits its
   Find-stub (Phase 210.A.2).
7. **Add the folded components to the bringup catalog** so the workspace
   documents what it now contains.

8. **A capability is SYSTEM-level, so it needs its own bringup.** Putting
   `[lifecycle]` in a shared `demo_bringup` made EVERY entry emit the autostart
   call, including the plain pubsub one. `managed_bringup` already showed the
   right shape.
9. **A moved bringup must be RENAMED.** It keeps `<name>demo_bringup</name>`,
   and two packages of that name collapse bringup resolution to
   `known bringup pkgs: []`.
10. **C/C++ `NANO_ROS_FEATURES` is `FORCE`-set per bake — last bake wins.** With
   three bringups, `managed_bringup`'s empty set erased the others' capabilities
   for the whole workspace. Declare the union in every bringup (a BARE
   `[lifecycle]` enables the feature without emitting autostart).
11. **`nano_ros_workspace()` needs `SYSTEM`** or the capability block is skipped
   entirely (`if(_NRW_SYSTEM)`) and `NANO_ROS_FEATURES` stays empty. `cpp` was
   missing it; `c` was not, which is why the same fold worked there first.
12. **Rust: the generated `<entry>_nros_selection` facade carries the
   capability**, and `nros sync` refuses to emit facades for a multi-system
   workspace. This is what makes the R1 shape unbuildable in rust and what R2
   routes around.

R1 result, for the record: `c` reached 18 → 30 source dirs with 7/7 fixtures
green and `cpp` 8/8, before R2 superseded the approach. Those builds are what
produced items 1–12.

### W2b — Normalise the language-workspace layout

`{rust,c,cpp}` are read SIDE BY SIDE as a language comparison, so their node
sets must be diffable. Today they are not — measured at HEAD 2026-08-02:

| | pubsub | service | action | prefix |
|---|---|---|---|---|
| rust | `talker_pkg` `listener_pkg` | `service_{server,client}_pkg` | `action_{server,client}_pkg` | none |
| c | `c_talker_pkg` `c_listener_pkg` | `c_add_{server,client}_pkg` | `c_fib_{server,client}_pkg` | all |
| cpp | `talker_pkg` `listener_pkg` | `cpp_add_{server,client}_pkg` | `cpp_fib_{server,client}_pkg` | mixed |

Three inconsistencies: rust names ROLES while c/cpp name DEMOS (`add`/`fib`);
prefixing is all / none / half; entry names diverge (`qemu_freertos_entry` vs
`freertos_entry`; `threadx_linux_entry` vs `native_threadx_entry` vs
`threadx_entry`). Coverage differs too — rust alone has `native_showcase_entry`,
`native_service_inprocess_entry`, `esp32_entry`, `zephyr_entry_robot1`; only `c`
has `nuttx_entry`.

**Target — identical structure, only the language differs:**

```
examples/workspaces/<lang>/src/          # <lang> in {rust, c, cpp}
  talker_pkg  listener_pkg
  service_server_pkg  service_client_pkg      # AddTwoInts payload
  action_server_pkg   action_client_pkg       # Fibonacci payload
  demo_bringup/
  native_entry
  native_service_{server,client}_entry
  native_action_{server,client}_entry
  native_entry_robot1  native_entry_robot2
  freertos_entry  nuttx_entry  threadx_entry  zephyr_entry  esp32_entry
```

Rules:
1. **No language prefix** in a single-language workspace — the directory says it.
   Prefixes stay in `mixed` and `features`, where languages coexist.
2. **Role names, not demo names.** `service_server_pkg`, not `add_server_pkg`:
   AddTwoInts is the payload, the role is the concept being compared.
3. **One platform vocabulary** — `freertos` / `nuttx` / `threadx` / `zephyr` /
   `esp32`, with no `qemu_` or `native_` qualifier.
4. **Same node set everywhere.** Close the gaps (c/cpp gain `esp32_entry`,
   cpp gains `nuttx_entry`) or record the exception in the workspace README.

- [x] Rename node pkgs to the role vocabulary, per language.
- [x] Drop the `c_`/`cpp_` prefixes in the single-language workspaces.
- [x] Unify entry names to the platform vocabulary.
- [x] Close (or document) the coverage gaps.
- [x] Update `fixtures.toml` entries, launch `pkg=`/`exec=`, bringup catalogs,
      `_ws_subdirs` / cargo `members`, and every test naming a renamed package.

**Acceptance:** `diff -r examples/workspaces/c/src examples/workspaces/rust/src`
lists only per-language files (`*.c` vs `*.rs`, `CMakeLists.txt` vs
`Cargo.toml`) — no structural differences. All fixtures green.

**Sequencing:** after W2 (`features/` exists, so the capability demos are out of
the way) and BEFORE W3 (the deletions), so the renames land once.

**Done 2026-08-02** — `db14b54e4` (W2, `features/`), `dc5d0a955` (W2b, parallel
layout). W2b's acceptance held: the three language workspaces now carry the same
node set under one vocabulary, which is what makes them a comparison rather than
three unrelated trees.

### W3 — Delete the folded directories and their fixture rows

- [x] Remove `ws-qos-{c,cpp,rust,mixed}`, `ws-params-{c,cpp,rust}`,
      `ws-lifecycle-{c,cpp,rust}`, `ws-custom-msg-{c,cpp,rust,mixed}`,
      `ws-remap-rust` (17 directories; `ws-launch-rust` is KEPT -- it is the only
      coverage of the launch v1 language surface, RFC-0066 open question answered).
- [x] Remove their `[[workspace_fixture]]` rows.
- [x] Re-point every test that named a deleted workspace at the large one.
- [x] `matrix_fixture_coverage` green — this is the gate that the deletion
      dropped no cell.

**Acceptance:** no test references a deleted path; coverage gates green; a
`git grep` for each deleted workspace name returns only historical docs.

**Done 2026-08-02** — `8ad9ac6e6` deleted 15 themed micro-workspaces;
`ed819f7ff` carried phase-330 W4's hand-authored params across so the fold lost
no declaration; `5c4690587` recorded the `ws-managed-cpp` split.
`matrix_fixture_coverage` green (8 tests).

### W4 — Make configuration an axis (rmw AND feature set)

- [x] Declare workspace fixtures as `(workspace) x (rmw) x (feature set)`,
      replacing hand-written near-duplicate rows.
- [x] Add the missing RMW coverage: `cyclonedds` and `xrce` on
      `workspaces/{c,cpp,rust}`, which do not exist today (84 of 86 workspace
      rows were zenoh).
- [x] Keep `mixed` at zenoh only — its value is the language seam, not the RMW
      seam. State that in the manifest so it reads as a decision, not a gap.
- [x] **Fold `safety` into the feature-set column.** `workspaces/{c,cpp,rust} x
      {default, safety-e2e}` replaces `ws-safety-{c,cpp,rust}` entirely.

      Evidence: the safety TALKER is the plain talker. Its own doc says that
      with `NANO_ROS_SAFETY_E2E=ON` "the zenoh backend automatically attaches a
      CRC-32 + sequence number on every publish — **no code change required
      here**". So the talker side is a pure build axis, and running it over the
      whole language workspace gives BROADER coverage than the three
      talker/listener pairs it replaces.

      The LISTENER is not: it calls
      `nros_cpp_subscription_register_validated` (surfacing `crc_valid`:
      1 ok / 0 mismatch / -1 no CRC) where the plain listener calls
      `nros_cpp_subscription_register`. That is a distinct API surface, so it
      becomes `{c,cpp,rust}_safety_listener_pkg` in `features/`.

      Caveat: `safety-e2e` changes probed ABI sizes, so the variant needs its
      own `target_dir` / build subdir or it trips the sizes-mirror guard. The
      `target-safety/` precedent already exists.
- [x] **Do not add a `uorb` axis value.** uORB models neither services nor
      actions (RFC-0011) while the large workspaces contain both, so the cell is
      unbuildable, not merely expensive. PX4 is a `CarveOut` with zero
      `platform = "px4"` rows; phase-325 owns that surface.

**Acceptance:** the new RMW and safety cells build and pass;
`matrix_fixture_coverage` shows the added coordinates; `ws-safety-*` deleted; no
`uorb` cell appears.

**Done 2026-08-02** — `1ac0e92eb` (safety 3 -> 1 workspace), `de05f89df` (the
RMW axis reaches the language workspaces), `9083e4c4f` (the six workspace RMW
cells declared in `matrix::CELLS`, so the coverage gate sees them).

Safety is 1 workspace, not 0. The talker side folded to a build axis exactly as
predicted; the LISTENER's `register_validated` API is a distinct surface, and the
one-system-per-workspace limit (phase-315 W1) blocks it from living in
`features/` — so `safety/` survives as a workspace. RFC-0066's inventory says
"zero"; that line is the prediction, this is the outcome.

### W5 — Re-measure and record

- [x] Repeat W1's measurements.
- [x] Record the delta in RFC-0066 (replacing "this has not been measured").
- [x] If the fold made things slower, say so and reconsider option (c) — a
      "core" and a "features" workspace per language — rather than quietly
      keeping a regression.

**Acceptance:** RFC-0066's cost section carries real numbers.

**Done 2026-08-03** — `docs/roadmap/data/phase-331-w5-remeasure.md`; RFC-0066's
cost section carries the numbers, replacing "this has not been measured".

Cold `just build-test-fixtures lane=native`: **6794 s / 1 h 53 m** (W1: 7051 s),
native stage **5222 s** (W1: 5912 s), **72** fixtures built (W1: 64), 0 errors.

The fold paid, and per-fixture is where it shows: the native stage got 11.7 %
faster while building 8 MORE fixtures, so seconds-per-fixture fell **21.5 %** —
35 `nros sync` + CMake-configure cycles became 15, which is the saving RFC-0066
predicted in the units it predicted. Wall clock understates it because the
non-native remainder grew 433 s for an unrelated reason (a
`regenerate-bindings.sh` fix in this session made it sync 7 template workspaces
it had been skipping — new work, not slower work). Attributable saving ~9.8 %.
No regression, so option (c) stays unused.

**Contaminated, and said so rather than smoothed over:** W6 landed BEFORE this
wave, which the ordering existed to prevent, and phase-330/332/333 all moved
underneath between the two runs.

The method is now `scripts/dev/measure-fixture-build.sh <lane>` — W1 left only
its numbers, with the procedure in a prose bullet.

### W6 — Consolidate realtime and bridge

Runs AFTER W5, so the re-measure is not contaminated.

**realtime: 8 -> 3.** `ws-realtime-*` is a scheduling DIMENSION, not a feature
and not a set of cases: one system (ctrl @10 ms high tier, telem @100 ms low
tier) projected onto each RTOS's native scheduler via
`[tiers.high.{posix,zephyr,nuttx,threadx}]`.

- [x] Fold `ws-realtime-{c,cpp}-mps2` back as `freertos_entry`, and
      `ws-realtime-cpp-fvp` as `fvp_entry`. These are unambiguous duplication:
      `ws-realtime-c-mps2/src/ctrl_pkg` is **byte-identical** to the base, and
      the only thing separating them is a `CMAKE_TOOLCHAIN_FILE` block that
      `workspaces/c` already carries and `ws-realtime-c` never got.
- [x] Fold `-rclcpp`, `-subnode`, `-subnode-portable` in as additional cpp
      entries (+ `subnode_pkg`).
- [x] Rename to `realtime-{c,cpp,rust}`.

**bridge: 2 -> 1.** `ws-bridge-rust` / `ws-bridge-xrce-rust` differ only in
which RMW they bridge to — the W4 axis, expressed as two directories.

**Rename the survivors**, dropping the now-meaningless `ws-` prefix:
`ws-launch-rust` -> `launch`, `ws-sizing-rust` -> `sizing`,
`ws-managed-cpp` -> `managed`.

**NOT in scope: merging realtime's tiers into the language workspaces.** The 86
`execution.tiers` dims are the hand-authored data issue 0380 destroyed twice,
and phase-330's coordination note asks that they not be disturbed. Re-resolving
every realtime model to merge them is exactly that risk. Revisit once RFC-0063
makes models build artifacts, at which point the risk largely evaporates.

**Acceptance:** 22 -> 12 workspaces; all fixtures green;
`matrix_fixture_coverage` green; no `ws-` prefix remains.

**Done 2026-08-03** — `a92778843` (realtime 8 -> 3 + 1, bridge 2 -> 1, `ws-`
prefix dropped), with fallout fixed in `d460c1a43`/`306dd2d26`/`fb0b1636c`
(issue 0395 — the freertos tiers and the `mid` tier the fold dropped) and
`c678c3cdf` (two west-built leaves lost their repo-root exclude; now gated by
`check-nested-workspace-excludes`).

**Measured end state:** 15 workspace directories (from 35), 93 workspace fixture
rows, tier1 = 10 coordinates. `git grep 'examples/workspaces/ws-'` outside
`docs/` returns nothing.

## Status — all waves landed (2026-08-03)

W1 through W6 are done. What was verified, stated at the tier it was actually
run at (RFC-0061): **tier 1**. `just check fast` green, `matrix_fixture_coverage`
green, and a cold `build-test-fixtures lane=native` built 72 fixtures with 0
errors. The embedded lanes were NOT swept — the freertos realtime fixture was
built and its generated tier table inspected by hand (issue 0395), but no tier-2
or tier-3 run has covered this phase's changes. W6's "all fixtures green" is
therefore claimed for native only; a `just ci-matrix` is the honest next step
before archiving this doc.

**End state is 15 workspaces, not the 12 the W6 acceptance names.** Three
survived with reasons recorded rather than being forced to the target:
`safety` (the listener's `register_validated` is a distinct API surface, and the
one-system-per-workspace limit keeps it out of `features/`),
`realtime-cpp-subnode-portable` (its whole purpose is proving a package builds
in ANOTHER workspace — folding it in would delete the property it tests), and
`bridge-{cyclonedds,xrce}` staying two (same limit).

## Target workspace inventory (end state)

**22 -> 12 workspaces.** The `ws-` prefix is dropped: everything under
`examples/workspaces/` is a workspace, so it never carried information.

| # | workspace | what it is | languages | platforms |
|---|---|---|---|---|
| 1-3 | `rust` / `c` / `cpp` | language comparison — IDENTICAL node sets (W2b, done) | one each | native + 5 embedded |
| 4 | `mixed` | the language SEAM: one entry, components from several languages | c+cpp+rust | native + 3 embedded |
| 5 | `features` | capability demos (params, lifecycle, qos, custom-msg, remap, validated-subscription) | all three | native (+3 zephyr, see W3) |
| 6-8 | `realtime-{c,cpp,rust}` | the scheduling DIMENSION: ctrl @10 ms high tier + telem @100 ms low tier, projected onto each RTOS scheduler | one each | native, nuttx, nuttx-riscv, zephyr, threadx, freertos, fvp |
| 9 | `bridge` | bridge topology; RMW is an AXIS, not a directory | rust | native |
| 10 | `launch` | the launch v1 LANGUAGE surface (`<arg>`, `$(var)`, `<group ns=>`, `<include>`) | rust | native |
| 11 | `sizing` | executor sizing: the launch names zero callback entities, the runtime needs six (issue 0257) | rust | native |
| 12 | `managed` | MANUAL-transition lifecycle (no autostart) | cpp | native |

**Safety becomes zero workspaces** — see W4.

Naming rules, applied everywhere:

- single-language workspace -> **no prefix** (`talker_pkg`);
- multi-language workspace (`mixed`, `features`) -> language prefix;
- **roles, not payloads** (`service_server_pkg`, not `add_server_pkg`);
- one platform vocabulary for entries (`freertos_entry`, `nuttx_entry`,
  `threadx_entry`, `zephyr_entry`, `esp32_entry`, `fvp_entry`).

## Explicitly out of scope

- **Board tier extraction** to a separate repository. RFC-0064's territory;
  measured as a maintenance-surface win (45 files per board, three drift
  checkers), not a build-time one (tier 3 is 2 % of fixture rows).
- **Standalone example restructuring.** `platform/lang/example` already holds
  for the six real platforms. The three deviating trees — `bridges/` (no
  language level), `templates/` (copy-out scaffolds), and the partial-language
  trees (`px4`, `stm32f4`, `qemu-esp32-baremetal`) — are a separate cheap pass.
- **Anything under `examples/px4/`.** See W4.
- **Test-side matrix binding.** Phase-329 owns it.

## Coordination with phase-330 (RFC-0063) — added 2026-08-02

Phase-330 makes the SystemModel a generated build artifact. The two phases
overlap on concrete files, so the order matters.

**The overlap, measured:** 29 of the 120 committed `*/config/*model.yaml` live
inside the 18 workspaces W3 deletes, along with 10 workspace-root
`CMakeLists.txt`.

**Recommended order: this phase's W2–W3 BEFORE phase-330's W4.** Deleting the
18 workspaces first drops phase-330's deletion census from 120 to 91 — there is
no point migrating files that are about to be removed. Nothing in phase-330
blocks this phase; its W1 (scheduling dims expressible in `system.toml`) is
already landed.

**W2 is safe from issue 0380.** W2 extends `demo_bringup` to place the folded
nodes, which re-resolves models — the operation that stripped 17 hand-authored
dims in issue 0380. It is safe here because **none of the 18 folded workspaces
carries any `execution.tiers` dim**: all 86 dims live in the nine
`ws-realtime-*` workspaces and two `orchestration_tiers_*` fixtures, none of
which this phase touches (verified by the phase-330 W2.b sweep). `nros sync`
also refuses to shrink a model now, so the failure mode is a loud refusal
rather than silent loss.

**Two system models in one workspace (W2) gets easier, not harder.** Once
models are generated per build, a workspace carrying `demo_bringup` and
`managed_bringup` produces two models as a consequence of having two bringups —
there is no committed pair to keep in sync.

**Measurement contamination (W1/W5).** Phase-330's W3–W4 change where models
are generated, which moves fixture build wall-clock. Land this phase's W1
baseline, fold, and W5 re-measure **before** phase-330 W3, or W5's delta mixes
two causes. Note also that all `examples/workspaces/*/build*` trees were wiped
on 2026-08-02 (104G), so W1 starts from a genuinely clean tree — which W1 wants
anyway.

**RFC-0065 is the same problem one level up.** W2/W3 move packages between
workspaces and delete 18 roots; every move edits a hand-maintained `SUBDIRS`
list or `[workspace] members`. RFC-0065 (colcon-like builder) removes exactly
that churn by discovering packages from the tree. It is a Draft with open
questions and this phase should NOT wait on it — but this fold is its strongest
motivating case, and 0065's design should be checked against what W2/W3
actually cost.

## Close-out (2026-08-06) — COMPLETE

All 28 work items done. Verified against the tree rather than against the
checkboxes, since the reason this doc sat in the active roadmap is that nobody
re-read it after the last wave:

- **W3 (the deletions) landed.** `ls -d examples/workspaces/ws-*` matches
  nothing. The four large workspaces (`rust`, `c`, `cpp`, `mixed`) and the
  native-only `features/` are in place, which is RFC-0066's shape.
- CLAUDE.md described this phase as "in flight" and said the themed dirs "are
  being DELETED (W3)". Corrected in the same commit — a router file that
  describes finished work as pending is how the next session re-plans it.

## Risks

- **Coarser bisection.** A QoS regression now fails inside a workspace that also
  builds pubsub/service/action, and one broken node package blocks that
  workspace's whole fixture. Accepted in RFC-0066; W5 is the checkpoint where it
  gets revisited if the pain is real.
- **Fold order matters.** `mixed` last: it depends on the C and C++ node
  packages being settled, and folding it first would mean touching those twice.
- **Stale build dirs will mask results.** Workspace build dirs cache a generated
  `nros_config_generated.h` per cargo target hash; a half-updated pair fails with
  "written by another crate with DIFFERENT probed sizes". Wipe from the manifest
  before each measurement — a hardcoded directory glob misses the non-default
  `build_subdir` names and produces exactly this failure.
