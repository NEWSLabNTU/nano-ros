---
id: 1841
title: "After a play_launch pin bump, the L3 queue job on a persistent self-hosted runner refuses at `just setup`: its workspace keeps the old submodule commit"
status: open
type: bug
severity: medium
area: [ci]
related: [1585, 1081, 1365]
found: 2026-10-12
---

## Measured

`a4958a4ad0` (#2038, 2026-10-12 07:30Z) moved `packages/cli/third-party/play_launch`
from `d23720c7` to `15258108`. Every L3 job since then that landed on a
persistent self-hosted runner failed within a minute, at `just setup`:

| merge_group run | job | runner |
| --- | --- | --- |
| 38228092853 (#2039 batch) | 114750057858 | nano-ros-runner-newslab-133 |
| 38232191325 (#2040 batch) | 114754098247 | nano-ros-runner-newslab-133 |
| 38233746970 (#2041 batch) | 114759218884 | nano-ros-runner-newslab-118-server |

```
setup: FAILED — packages/cli/third-party/play_launch is at d23720c7, not the pin this tree records,
  and it has uncommitted changes, so this cannot move it for you (issue 1081).
  ...
  Commit or stash the submodule work, then:  git submodule update packages/cli/third-party/play_launch
error: recipe `_setup-common` failed with exit code 1
```

The job's own `actions/checkout@v4` step (queue.yml, `submodules` left at
false) reports the state it leaves behind:

```
M	packages/cli/third-party/play_launch
 d23720c7557f78789e90de37dfd62c19c8a2532c packages/cli/third-party/play_launch (v0.15.1)
```

## What it is

The job workspace (`/home/runner/_work/nano-ros/nano-ros`) persists between
jobs on 118 and 133. A checkout without submodules moves the superproject
to the new commit and leaves the submodule's worktree at whatever an
earlier job built in it. Building `nros-launch-resolve` writes inside
that worktree; CLAUDE.md notes `src/play_launch_container` shows as
modified after a build, and that this is expected. Issue 1081's guard then
sees both a pin mismatch and a dirty submodule and, correctly for a
developer tree, refuses to move it.

This is issue 1585's class (resolved 2026-09-29) one directory over. 1585
was the runner-bootstrap STORE checkout leaving submodules behind; this is
the per-job WORKSPACE, which `runner-bootstrap.sh` does not touch.
The first three failures were on 118 and 133 only, so when this was filed
`nano-ros-runner-newslab-server-241` was taken to be unaffected, its
containers being ephemeral. **That was wrong.** Its ephemeral container
mounts a persistent workspace too, and it fails the same way:

| merge_group run | job | runner |
| --- | --- | --- |
| 38241201245 (#2045 batch, 11:12Z) | L3 | nano-ros-runner-newslab-server-241 |
| 38242035564 (#2047 batch, 11:20Z) | L3 | nano-ros-runner-newslab-133 |

```
setup: FAILED — packages/cli/third-party/play_launch is at d23720c7, not the pin this tree records,
```

So every self-hosted runner refuses at `setup`, and every L3 run since
the pin bump is red. That includes 38234480678, 38237477170 and
38240017467 on newslab-118.

## What it is not

- Not issue 1755 (the runner cache directory): the doctor step passes.
- Not issue 1829 or #2023: the job stops before `generate-bindings` and
  `rust-rtos-link-check`.
- Not a real developer change in the submodule. Nothing on a runner
  authors play_launch work, so the guard's advice ("commit or stash") has
  no meaning there.

## What would close it

A self-hosted job's workspace reaches the recorded play_launch pin on every
run, whatever an earlier job left in it. For example, a checkout step that
resets the submodule (`git -C packages/cli/third-party/play_launch reset
--hard && git clean -ffdx`, then `git submodule update`) before `just
setup`, or `submodules: true` with a clean. It is proven by an L3 job on
118 or 133 getting past `setup` after a pin bump. The 1081 guard should
stay as it is for developer trees.

## 2026-10-13 — fix: a CI workspace discards submodule residue; a developer tree still refuses

`_setup-common`'s inline guard moved to `scripts/lib/sync-cli-submodule.sh`,
which `just setup` runs for every scope, so every workflow that sets up
(queue L3, run-matrix, nightly) takes the same path with no per-workflow step
to forget. The three states:

| submodule state | developer tree | CI job (`GITHUB_ACTIONS=true`) |
| --- | --- | --- |
| not initialised | init | init |
| off pin, clean | update | update |
| off pin, dirty | **refuse** (issue 1081, unchanged) | list the dirt, `reset --hard`, `clean -ffd` (no `-x`: ignored build output is a cache), `submodule update --force` |

`GITHUB_ACTIONS` rather than a per-workflow opt-in: GitHub sets it on every
job and no developer shell has it, while an opt-in would need every
self-hosted job to remember it — which is the shape of this issue, one
persistent-workspace path past 1081's fix. The discarded paths are printed, so
the next log says what the residue was instead of leaving it inferred.

Gate `check cli-submodule-sync` runs the script's selftest: throwaway
repositories, the superproject pinned to commit two and the submodule left on
commit one with a modified tracked file and an untracked one.

- Fixed: 3/3 cases ok.
- Mutation, CI arm disabled (`elif false`, i.e. the old behaviour): the CI
  case fails (rc=1, still on the old commit, still dirty), selftest rc=1.
- Mutation, CI arm always taken (`elif true`): the developer case fails (rc=0,
  the modified file overwritten), selftest rc=1.
- Restored: rc=0.

**Not yet proven live.** The acceptance above is an L3 job on a persistent
runner getting past `setup` after a pin bump. The runners still sit on
`d23720c7` against pin `15258108`, so the first L3 run carrying this change is
that test; this issue closes on its log.
