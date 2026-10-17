---
id: 1800
title: "Two ROS 2 tests that are not interop cells are run by no scheduled lane: `workspace_shadowing` and `cyclone_slirp_pair`"
status: open
type: tech-debt
area: [ci, testing]
severity: low
found: 2026-10-11
related: [1758, 1251, 1127, phase-505]
---

**Home: [phase-505](../roadmap/phase-505-ci-lanes-reach-and-infra-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## Measured

Issue 1758 made a missing prerequisite a failure and moved the ROS 2 question
into the lane's scope. Tiers 1, 2 and the nightly declare `NROS_TEST_UNCLAIMED=ros2`,
so they deselect every test that calls `lane_scope::require_ros2_claimed()`.
Live-peer is the lane that claims ROS 2, but it runs only `interop::CELLS`
rows with a recorded PASS in `.config/interop-verdicts.toml`. Two ROS 2 tests
are in neither set, so only `just ci full` (tier 3) selects them, and no
schedule runs tier 3.

Before 1758 both "passed" in tier 1 by skipping, so no coverage was lost. The
gap is only visible now.

### `workspace_shadowing`

It checks that a workspace `std_msgs` shadow (carrying `Marker.msg`) wins over
the ament-installed one, by `nm` on the prebuilt `shadowing` compile-check
fixture. It needs an ament layer that ships `std_msgs`; it starts no ROS 2
process and talks to no peer. That is why it does not fit `interop::CELLS`:
an `InteropCell` names a `Peer` and a `Dir`, and this test has neither.

The live-peer job's image (`ghcr.io/newslabntu/nano-ros-ci:humble`) has the
ament layer, so the test could run there if the lane selected it.

### `cyclone_slirp_pair`

Issue 1251's probe of the Cyclone `<Discovery><Tag>` profile behind libslirp.
It needs ROS 2 with `rmw_cyclonedds_cpp`, `unshare` and `slirp4netns`,
unprivileged user and network namespaces, and the Cyclone backend's own test
tree (`just cyclonedds build-rmw`). Two measurements against the self-hosted
runner image (`nano-ros-runner:local`, run with the flags
`runner-container.sh --run` uses: `--cap-drop ALL --security-opt
no-new-privileges --user runner`):

```
unshare: unshare failed: Operation not permitted
```

and `slirp4netns` is not on PATH. So no containerized lane can run it without
loosening the runner's security, which `runner-container.sh` refuses to do on
purpose. It also does not fit `interop::CELLS`: its guest is the backend's
test binary, built by no `BuildChannel`. `interop::SLIRP_PAIR_CELLS` is for
cells whose nano side runs behind slirp, and it is empty.

## Options

1. **Live-peer runs a declared list of non-cell ROS 2 tests** beside the
   ledger. This works for `workspace_shadowing` only. The cost is a second way
   into the lane besides the ledger.
2. **A `Peer`-less row in `interop::CELLS`** (an ament-only cell). This keeps
   one way in but stretches the cell model to something that is not interop.
3. **Leave both in tier 3** and run them by hand before a release:
   `cargo nextest run -p nros-tests --test workspace_shadowing --test
   cyclone_slirp_pair` on a host with ROS 2.
   `cyclone_slirp_pair` stays here whichever option is chosen, until a lane
   runs on a host (not a container) that permits user namespaces.

## Decision

Option 1 for `workspace_shadowing`. `scripts/check-interop-verdicts.py` holds
`HOST_ROS2_TESTS`, a declared list of `(binary, case)` beside the ledger. The
live-peer host job builds what each case resolves (`--host-compile-checks`, so
the `shadowing` fixture), runs it (`--filters`) and fails the lane as "could not
run" if it produced no pass or fail (`--assert-ran`). The case records no
ledger verdict, because it was never an interop measurement. Measured locally
on the `shadowing` fixture against `/opt/ros/humble`: PASS in 0.386 s, and the
assert reads 1 of 1.

`cyclone_slirp_pair` stays in tier 3 and is run by hand (option 3). This issue
stays open for it until a lane runs on a host that permits user namespaces.
