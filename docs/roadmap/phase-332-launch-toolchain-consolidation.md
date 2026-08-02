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

- [ ] Repoint `packages/cli/third-party/ros-launch-resolve` at the
      `play_launch` repository (submodule URL + path)
- [ ] `packages/cli/nros-launch-resolve/Cargo.toml`: update the two path
      dependencies (`ros-launch-resolve`, `play_launch_parser`) to the new
      in-repo locations
- [ ] `just setup-launch-resolve` (justfile ~2784–2839): update the crate path
      it builds

**Acceptance:** `just setup-launch-resolve` succeeds on a machine with no ROS
sourced, and the resulting binary shows zero `rcl`/`rmw`/`ament` entries in
`ldd`. Then `just build-test-fixtures lane=native` — the multi-host fixtures
use `$(eval …)`, which routes to CPython, so a working resolver is the thing
under test.

## W2 — `ros-launch-manifest` by tag

- [ ] Depend on `ros-launch-manifest` by git tag rather than through the
      nested submodule path (`packages/cli/third-party/ros-launch-resolve/
      third-party/ros-launch-manifest`)
- [ ] Coordinate the tag with play_launch — both link these crates and must
      move together, or pin an older tag deliberately

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
