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
`nano-ros-runner-newslab-server-241` runs ephemeral containers with a
fresh workspace and is not affected, which is why the lane is not
uniformly red.

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
