---
id: 2386
title: "On a never-built workspace, a cross image's census step builds its host image before `nros configure` has stamped it, so the cross image cannot build"
status: open
type: bug
severity: medium
area: [build, census, cmake, cli]
found: 2026-11-10
related: [issue-2235, phase-466]
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md).**

## What

`nros build` on a C/C++ cross image (`action_bringup:freertos_action_server`
in `examples/workspaces/cpp`) takes a census of the code by building the
NATIVE image generated from the same launch file
(`action_bringup:linux_action_server`). On a workspace where that native image
was never configured, the census build's cmake configure fails:

```
nros build:   `action_bringup:freertos_action_server`: could not take its census
(building the host image `action_bringup:linux_action_server`; no census can be
taken without it: configure failed: `... NROS_LEAF_CENSUS_HOST=1 cmake -S
build/posix-zenoh-linux-action_bringup -B build/posix-zenoh-linux-action_bringup/cmake ...`
```

The configure's own error is from `nros_configured_locate`
(`cmake/NanoRosResolved.cmake:94`):

```
Error: `action_bringup:linux_action_server` in .../examples/workspaces/cpp has
not been configured (no .../build/nros/configured/action_bringup__linux_action_server/stamp.toml):
run `nros configure --image action_bringup:linux_action_server` there
```

The cross image's configure then refuses, because there is no census to check
the contract against (`NanoRosEntry.cmake:247`, "census missing").

So the census step depends on state that only a previous run of something else
left behind. A tree where the native image had been built before passes; a
clean one does not.

## Measured

- Reproduced on `main`'s code (2026-11-10, worktree at `201874c2c9`, no local
  changes to the build tooling): in `examples/workspaces/cpp`, delete
  `build/nros/models/action_bringup/*.census.json`,
  `build/nros/configured/action_bringup__linux_action_{server,client}` and
  `build/posix-zenoh-linux-action_bringup`, then
  `nros build action_bringup:freertos_action_server`: exit 2, with the two
  errors above.
- The same command passed on that tree before the deletion: a census and a
  configure stamp left by an earlier run satisfied it.
- Hit by `just build-test-fixtures lane=tier2` on a worktree whose root `build/`
  had been deleted to free disk. Every `action_bringup` cross image (FreeRTOS
  and NuttX, server and client) failed the same way, so tier 2's `_lane-gate`
  then reported 31 workspace and 28 compile-check fixtures missing.

## Where to look

The census take builds the host image with `NROS_LEAF_CENSUS_HOST=1 cmake`
directly. The host image's configure reads its configure stamp
(`nros_configured_locate`), which only `nros configure --image <host image>`
writes. The census take should configure the host image first (or take the
stamp's facts from the same plan that chose the host image), so it works from
a clean tree.

## Workaround

`nros configure --image action_bringup:linux_action_server` (and `_client`) in
the workspace before building the cross images.
