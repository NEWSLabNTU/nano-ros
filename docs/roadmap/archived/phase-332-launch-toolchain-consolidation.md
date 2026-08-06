# Phase 332 — Launch toolchain consolidation

**Implements:** RFC-0060 (amended — W0 below is the amendment; this phase
cannot start until it lands)
**Informed by:** the play_launch-side measurements of 2026-08-02
(`play_launch:docs/design/launch-toolchain-topology.md`), issue 0285
(PATH-resolved binaries), and the submodule drift RFC-0060 itself cites
**Counterpart:** play_launch phase-55 — W1 there must land before W1 here

> **Drafted from the play_launch side.** The measurements are ours; the
> decision is nano-ros's. If W0 is rejected, close this phase as wontfix —
> that is a legitimate outcome, not a blocker to argue past.

## Problem

RFC-0060 split the launch toolchain into three **repositories** along "what a
consumer must be able to link". The layering is right and is not in question.
The repository boundary is the part that is not paying for itself.

The isolation nano-ros actually depends on comes from two other boundaries:

- **Cargo workspace** — `play_launch`'s root manifest `exclude`s layer 2, so
  the `rclrs`/`rosidl_runtime_rs` patches never enter the resolver's graph.
- **Process** — the resolver is a binary, so `libpython` never enters the
  `nros` link. This is what keeps the shipped `nros` libc-only, and it is
  what `packages/cli/nros-cli-core/Cargo.toml` already documents.

Both survive folding layer 2 into the play_launch repository. What does not
survive is three-level submodule nesting (`play_launch` →
`ros-launch-resolve` → {`play_launch_parser`, `ros-launch-manifest`}), which
during the 2026-07-31 `machine=` removal produced a dropped pointer bump, two
agents racing on one submodule tree, and three commits whose only content was
moving a pointer. RFC-0060 cites the same class of failure as motivation for
the split; the split reproduced it one level deeper.

## Evidence (measured, not assumed)

From a clean clone with no `install/`, no `build/`, and the ROS environment
stripped:

| Check | Result |
|---|---|
| `rclrs`/`rosidl` in `cargo tree -p ros-launch-resolve-cli` | **0** of 294 deps |
| Build with ROS env stripped | **succeeds, 11.6s** |
| ROS/rcl/rmw/ament shared libs in the binary | **0** |
| `libpython3.10` linked | **yes** — required for `.launch.py` |
| Resolve a `.launch.xml`, no ROS sourced | **yes** |
| Resolve a `.launch.py`, no ROS sourced | **yes**, node present in the model |

The last row matters for the scan stage: `import launch` **fails** in that
environment, yet `.launch.py` resolves correctly. The parser supplies its own
`launch`/`launch_ros` API via pyo3 mock modules. **Layer 2 needs CPython, not
a ROS installation.**

Caveat: the fixture was self-contained. `$(find-pkg-share …)` still needs
`AMENT_PREFIX_PATH` at runtime — a launch-file semantics property, unchanged
by any of this.

## W0 — Amend RFC-0060 (BLOCKING, nano-ros's call)

RFC-0060 is Stable. This phase contradicts its repository count, so the RFC
moves first — per `AGENTS.md`, rationale lives in an RFC, never only in a
phase doc.

- [x] Amend RFC-0060 (2026-08-02): added the "## Amendment — two repositories,
      not three" section — keeps the three layers, the linking rule, and the
      process boundary; folds layer 2 into the play_launch repo as an
      `exclude`d cargo workspace; Status stays Stable.
- [x] `ARCHITECTURE.md` — no matching section exists (the 3-layer text lives in
      `CLAUDE.md`, which describes LAYERS, still accurate post-amendment; the
      repo count is not asserted there). No edit needed; the `CLAUDE.md`
      "pins only layer 2" mechanics update belongs to W1 (the actual repoint).
- [x] **Decision: ACCEPT** (maintainer, 2026-08-02). The four reject-reasons —
      release cadence, access control, CI cost, bigger pinned tree — were each
      considered and cleared; recorded in the RFC-0060 Amendment. None blocking:
      the recurring three-level-submodule drift cost outweighs the inert
      tree-size cost.

**W0 COMPLETE — play_launch phase-55 W1 is unblocked.**

## W1 — Repoint at the merged tree

Starts only after play_launch phase-55 W1 has landed, since the pinned commit
must already contain the merged directories.

- [x] Repointed: the submodule is now `packages/cli/third-party/play_launch`
      (url `github.com/NEWSLabNTU/play_launch`, pinned 402dfe9) — URL + path both
      changed. Init is NON-recursive (layer 2 = regular files; layer-3 runtime
      submodules stay uninitialised).
- [x] `nros-launch-resolve/Cargo.toml`: `ros-launch-resolve` →
      `../third-party/play_launch/src/ros-launch-resolve/resolve`,
      `play_launch_parser` → `.../src/ros-launch-resolve/parser/crates/play_launch_parser`.
- [x] `just setup-launch-resolve`: existence check, init hint, and the freshness
      probe (`git -C … ls-files src/ros-launch-resolve`, no `--recurse-submodules`)
      all repointed; `just doctor` (`workspace.just`) + `ws.rs` message + CLAUDE/
      AGENTS/README docs updated to the new path + non-recursive init.

**W1 verified** (2026-08-03, no ROS sourced): the isolated `nros-launch-resolve`
workspace builds — `ros-launch-resolve` + `play_launch_parser` from the merged
play_launch layout; `just setup-launch-resolve` + `just setup-cli` succeed.

**Acceptance:** `just setup-launch-resolve` succeeds on a machine with no ROS
sourced, and the resulting binary shows zero `rcl`/`rmw`/`ament` entries in
`ldd`. Then `just build-test-fixtures lane=native` — the multi-host fixtures
use `$(eval …)`, which routes to CPython, so a working resolver is the thing
under test.

## W2 — `ros-launch-manifest` by tag

- [x] All seven `ros-launch-manifest-{types,model,sched,check}` deps (nros-macros,
      nros-orchestration-ir, nros-cli-core, nros-tests) are now
      `{ git = "…/ros-launch-manifest.git", tag = "v0.1.0" }`; the nested rlm
      submodule and the last stray top-level vendored rlm copy are gone.
- [x] Tag coordinated: **v0.1.0** = the SAME tag play_launch pins → both resolve
      to rev `172aa538`, one copy. Both workspaces `cargo check --locked` green.

**W2 verified.** This closes phase-332 (W0+W1+W2). W3 in the play_launch phase-55
already dropped their rlm submodule; nano-ros no longer nests anything under the
launch pin — the three-level nesting that motivated this phase is gone.

This is the step that actually removes the nesting from nano-ros's side. W1
alone shortens the chain; W2 flattens it.

## Risk

**A bigger pinned tree.** Pinning `play_launch` means vendoring its C++
container, `play_launch_msgs`, and web UI alongside the resolver. Nothing
builds them — `cargo build -p ros-launch-resolve-cli` touches only layer 2's
workspace — but the checkout grows. If that is unacceptable, say so in W0;
it is a legitimate reason to keep the repository boundary and the honest
answer is then to reject rather than work around it.

**Ordering.** W1 here depends on play_launch phase-55 W1. Doing them in the
wrong order leaves a submodule pointing at a commit whose layout does not
match the Cargo paths.

## Close-out (2026-08-06) — COMPLETE

W0's amendment was accepted (two repositories, not three) and W1/W2 landed.
Verified in the tree:

- `packages/cli/third-party/play_launch` is the pinned repo.
- Layer 2 (the resolver, launch tree → SystemModel) is REGULAR FILES at
  `src/ros-launch-resolve`, not a nested submodule.
- `ros-launch-manifest` is a git-TAG cargo dep rather than a second vendored
  copy — the issue-0285 double-vendoring is gone, and with it the `--recursive`
  landmine.

CLAUDE.md already carries the landed shape, including the non-recursive init and
the absolute-path rule for `nros-launch-resolve` (issue 0285). Nothing here
outlives the archive.
