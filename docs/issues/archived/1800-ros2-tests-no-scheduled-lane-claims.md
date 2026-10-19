---
id: 1800
title: "Two ROS 2 tests that are not interop cells are run by no scheduled lane: `workspace_shadowing` and `cyclone_slirp_pair`"
status: resolved
type: tech-debt
area: [ci, testing]
severity: low
found: 2026-10-11
resolved: 2026-10-19
resolved_in: "branch fix/phase-505-1800"
related: [1758, 1251, 1127, phase-505]
---

**Home: [phase-505](../../roadmap/phase-505-ci-lanes-reach-and-infra-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

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

## Progress (2026-10-18)

**`workspace_shadowing` has a lane.** PR #1942 landed option 1, and the
scheduled live-peer lane runs it every night. In run 39096469491
(2026-10-18) the host job's `== workspace_shadowing ==` step reads:

```
PASS [   0.296s] (1/1) nros-tests::workspace_shadowing workspace_std_msgs_shadows_ament_in_consumer_binary
```

**What keeps this issue open is `cyclone_slirp_pair` alone.** Its blocker is
infrastructure, not code: it needs a lane on a host, not a container, that
permits unprivileged user and network namespaces and has `slirp4netns`. Every
CI lane here runs in a `--cap-drop ALL` container that refuses both. Until such
a host exists, it runs by hand before a release, which is option 3 above.

## Resolution (2026-10-19)

**`cyclone_slirp_pair` joins live-peer too, so both tests have a scheduled
lane.** The blocker named above was real for the SELF-HOSTED runner and wrong
for live-peer. Live-peer's host job (`regression`, "rows whose board IS this
runner") runs on a GitHub-hosted `ubuntu-22.04` VM, inside an ordinary Docker
container of `nano-ros-ci:humble`, not inside `runner-container.sh`'s
`--cap-drop ALL` container. A default Docker container still refuses the test,
for two reasons, both measured on `ubuntu:22.04`:

- Docker's default seccomp profile allows `unshare` only with
  `CAP_SYS_ADMIN`: `unshare --user --map-root-user --net true` gives
  `unshare: unshare failed: Operation not permitted`. With
  `--security-opt seccomp=unconfined` it exits 0. No capability is added.
- `slirp4netns` opens `/dev/net/tun`, which a default container does not
  have (`open("/dev/net/tun"): No such file or directory`). With
  `--device /dev/net/tun`, a non-root user gets a `tap0` with
  `10.0.2.100/24` and a ping to `10.0.2.2` answers.

What changed:

- `live-peer.yml`: the `regression` job's container gets
  `options: --security-opt seccomp=unconfined --device /dev/net/tun`. The VM
  is single-use, so this loosens nothing that outlives the run. The
  self-hosted runner container still refuses both, on purpose.
- `nros-sdk-index.toml`: `[prereq.slirp4netns]` and `[prereq.iproute2]`
  (role `infra`). The image has neither: `ros:humble-ros-base` has `unshare`
  but no `ip` and no `slirp4netns`. The job's existing
  `nros setup --system --sudo` step installs them from the index.
- `scripts/check-interop-verdicts.py`: `HOST_ROS2_TESTS` names
  `cyclone_slirp_pair::a_tag_isolated_host_profile_reaches_a_slirp_guest_and_nothing_else`.
  `--host-compile-checks` derives the guest row `cyclone_slirp_guest` (issue
  1903) from the test source, and `--assert-ran` fails the lane if the case
  produces no result. The self-test now covers a list of more than one entry.

Cost, measured locally:

- The `cyclone_slirp_guest` row (static Cyclone plus the backend's
  `ros2_pub`) builds in 46 s on 4 CPUs, and its output is 15 MB.
- The test takes 44.6 s. Its two negative controls spend their full 12 s
  windows.
- The two apt packages are small.

That is about 2 minutes added to a lane that already takes 70 to 100 minutes.

Evidence, local, on Ubuntu 22.04 with `/opt/ros/humble`: the row built through
`NROS_FIXTURE_ID=cyclone_slirp_guest scripts/build/compile-check-fixtures.sh`,
then the lane's own filter ran it:

```
PASS [  44.583s] (1/1) nros-tests::cyclone_slirp_pair a_tag_isolated_host_profile_reaches_a_slirp_guest_and_nothing_else
```

`workspace_shadowing` keeps running there. See the 2026-10-18 progress note
above (run 39096469491). The first scheduled run after this lands is the first
CI measurement of `cyclone_slirp_pair`. A red there is a red, not a
regression, because the case records no ledger verdict.
