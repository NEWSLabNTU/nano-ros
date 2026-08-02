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
- [ ] Per-workspace `nros sync` + CMake-configure time for the four large
      workspaces and for a representative themed one.
- [ ] Record cell counts from `lane-coords tier1 --cells` and the
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

- [ ] Rename node pkgs to the role vocabulary, per language.
- [ ] Drop the `c_`/`cpp_` prefixes in the single-language workspaces.
- [ ] Unify entry names to the platform vocabulary.
- [ ] Close (or document) the coverage gaps.
- [ ] Update `fixtures.toml` entries, launch `pkg=`/`exec=`, bringup catalogs,
      `_ws_subdirs` / cargo `members`, and every test naming a renamed package.

**Acceptance:** `diff -r examples/workspaces/c/src examples/workspaces/rust/src`
lists only per-language files (`*.c` vs `*.rs`, `CMakeLists.txt` vs
`Cargo.toml`) — no structural differences. All fixtures green.

**Sequencing:** after W2 (`features/` exists, so the capability demos are out of
the way) and BEFORE W3 (the deletions), so the renames land once.

### W3 — Delete the folded directories and their fixture rows

- [ ] Remove `ws-qos-{c,cpp,rust,mixed}`, `ws-params-{c,cpp,rust}`,
      `ws-lifecycle-{c,cpp,rust}`, `ws-custom-msg-{c,cpp,rust,mixed}`,
      `ws-remap-rust` (17 directories; `ws-launch-rust` is KEPT -- it is the only
      coverage of the launch v1 language surface, RFC-0066 open question answered).
- [ ] Remove their `[[workspace_fixture]]` rows.
- [ ] Re-point every test that named a deleted workspace at the large one.
- [ ] `matrix_fixture_coverage` green — this is the gate that the deletion
      dropped no cell.

**Acceptance:** no test references a deleted path; coverage gates green; a
`git grep` for each deleted workspace name returns only historical docs.

### W4 — Make configuration an axis

- [ ] Declare workspace fixtures as `(workspace) × (rmw) × (feature set)` per
      RFC-0066, replacing hand-written near-duplicate rows.
- [ ] Add the missing RMW coverage: `cyclonedds` and `xrce` on `workspaces/
      {c,cpp,rust}`, which do not exist today.
- [ ] Keep `mixed` at zenoh only (its value is the language seam, not the RMW
      seam) — state that in the manifest so it reads as a decision, not a gap.
- [ ] **Do not add a `uorb` axis value.** uORB models neither services nor
      actions (RFC-0011), and the large workspaces contain both; the cell is
      unbuildable, not merely expensive. PX4 stays out of this phase entirely —
      it is a `CarveOut` with zero `platform = "px4"` fixture rows, so it
      contributes nothing to the time being reduced. Phase-325 owns that surface.

**Acceptance:** the new RMW cells build and pass; `matrix_fixture_coverage`
shows the added coordinates; no `uorb` cell appears.

### W5 — Re-measure and record

- [ ] Repeat W1's measurements.
- [ ] Record the delta in RFC-0066 (replacing "this has not been measured").
- [ ] If the fold made things slower, say so and reconsider option (c) — a
      "core" and a "features" workspace per language — rather than quietly
      keeping a regression.

**Acceptance:** RFC-0066's cost section carries real numbers.

## Target workspace inventory (end state)

32 workspace directories become **13**:

| # | workspace | shape | languages | platforms |
|---|---|---|---|---|
| 1-3 | `rust` / `c` / `cpp` | language comparison — IDENTICAL node sets (W2b) | one each | native + 5 embedded |
| 4 | `mixed` | the language SEAM: one entry, components from several languages | c + cpp + rust | native + 3 embedded |
| 5 | `features` | capability demos (params, lifecycle, qos, custom-msg, remap) | all three, prefixed | **native only** |
| 6-8 | `ws-safety-{c,cpp,rust}` | `safety-e2e` build feature; cross-process pair | one each | native |
| 9-11 | `ws-realtime-{c,cpp,rust}` (+ `-rclcpp`, `-subnode`, `-subnode-portable`, `-mps2` variants) | own toolchain pin; the only multi-platform theme | one each | native, nuttx, nuttx-riscv, freertos |
| 12 | `ws-sizing-rust` | executor sizing: launch names zero callback entities, runtime needs six (issue 0257) | rust | native |
| — | `ws-launch-rust` | the launch v1 LANGUAGE surface | rust | native |
| — | `ws-bridge-rust`, `ws-bridge-xrce-rust` | bridge topology; the only non-zenoh workspace rows | rust | native |
| 13 | `ws-managed-cpp` | MANUAL-transition lifecycle. Split out of `features/` in W2: a second bringup there would make it two-system and break rust's selection facades (the limit is per WORKSPACE, not per language) | cpp | native |

(The last three rows are the remaining outliers; counting them individually the
total is 15 directories, 12 distinct *shapes*.)

Naming rules, applied consistently:

- **single-language workspace** -> no prefix (`talker_pkg`);
- **multi-language workspace** (`mixed`, `features`) -> language prefix
  (`c_talker_pkg`), because the languages coexist;
- **roles, not payloads** (`service_server_pkg`, not `add_server_pkg`);
- **one platform vocabulary** for entries (`freertos_entry`, `nuttx_entry`,
  `threadx_entry`, `zephyr_entry`, `esp32_entry`).

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
