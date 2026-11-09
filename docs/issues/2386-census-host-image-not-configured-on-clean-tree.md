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

- Reproduced on `main`'s code (2026-11-10, worktree at `a877dac37d` (#3369), no local
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

## Fix (2026-11-10)

There were two gaps, and the second stays hidden until the first is fixed.

1. **The host image had no configure stamp.** Stamps are written at the end of
   a configure's plan loop, and the census take builds the host image in the
   middle of it. `configure::record_planned` now records an image's stamp from
   the plan just made for it, with the same facts the end-of-loop stamps
   carry. `planned_facts` is the one helper both use. The census take calls
   it for the host image before building it.
2. **A shared cmake configure checks every member's census.** The FreeRTOS
   server and client images share one coordinate (phase-530 W3), so
   configuring the server also configures `freertos_action_client_entry`, and
   that checks the client's census. `nros build` took the census for the asked
   image only. Stage 3.4 now takes it for every member of the image's cmake
   coordinate, by the same `coordinate_members` predicate the generated root
   and the union resolve use, and once per plan.

Measured on the reproduction above, with the censuses, configured stamps and
host build dir for both linux action images moved aside:

| | `nros build action_bringup:freertos_action_server` |
| --- | --- |
| before the fix | exit 1: "has not been configured", then "census missing" |
| fix 1 only | the server census is taken; the configure then stops on the client's "census missing" |
| both fixes | exit 0; both censuses taken (server 1 node / 2 pub / 3 srv; client 1 node / 1 sub / 3 cli) and the cross image built |

Test: `a_stamp_recorded_from_a_plan_is_current_to_a_default_configure`.

Not addressed here: ament-workspace-port's different failure (its
SystemModel does not resolve).
