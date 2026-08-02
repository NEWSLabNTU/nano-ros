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

- [ ] Wall-clock `just build-test-fixtures lane=native` on a clean tree
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

### W2 — Fold the API themes into the large workspaces

One workspace at a time, `c` first (smallest node set), then `cpp`, `rust`,
`mixed`.

- [ ] Move `qos_{talker,listener}_pkg`, `param_talker_pkg`,
      `lifecycle_talker_pkg`, `custom_msgs/`, `reading_{talker,listener}_pkg`,
      `remap_talker_pkg` into `examples/workspaces/<lang>/src/`.
- [ ] Move `managed_bringup` into `workspaces/cpp` — the workspace then carries
      **two system models**; confirm `nros codegen-system` handles both and that
      the entry packages select the right one.
- [ ] Extend `demo_bringup` to place the folded nodes.
- [ ] Confirm `custom_msgs` stays workspace-local (RFC-0066 open question) —
      four copies of an interface package must not collide.
- [ ] Verify `ws-launch-rust`'s coverage is genuinely carried by the large
      workspaces' bringups **before** deleting it (RFC-0066 open question).

**Acceptance:** each large workspace builds and its e2e tests pass; the folded
nodes are placed by a bringup and observable at runtime, not merely compiled.

### W3 — Delete the folded directories and their fixture rows

- [ ] Remove `ws-qos-{c,cpp,rust,mixed}`, `ws-params-{c,cpp,rust}`,
      `ws-lifecycle-{c,cpp,rust}`, `ws-custom-msg-{c,cpp,rust,mixed}`,
      `ws-remap-rust`, `ws-launch-rust` (18 directories).
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
