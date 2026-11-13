---
id: 2497
title: "Moving an SDK pin stops the self-hosted runner's loop, and nothing turns that into a red"
status: open
type: bug
area: ci, runner, zephyr
related: [issue-2389, issue-1279]
found: 2026-11-12
---

## What happened

On 2026-11-12 at 15:27:48Z the self-hosted runner on host 118
(`nano-ros-runner-newslab-118-server`, labels `nros-sdk-zephyr,nros-big`) started
its next ephemeral container, and the container exited 78: the label gate
refused to register. `scripts/ci/runner-loop-container.sh` treats 78 as
"restarting cannot help" and exits, which is correct for a broken host. Every
L3 job queued after that waited with no runner and no failure. One of them,
run 43247694051, was still queued the next morning.

A read-only doctor run against the runner's own volumes, in a throwaway
container with `--network none`, found two problems:

    [MISSING] west is not on PATH
    [MISSING] Zephyr SDK 0.16.9 is not registered with cmake

The store held `zephyr-sdk/0.16.8` only. The 3.7 line had moved to v3.7.2, whose
`zephyr/SDK_VERSION` asks for 0.16.9 (issue 2389). Nothing on the host was
broken: the store was one pin behind.

## Why it stayed down

There were two independent gaps.

1. **`just zephyr setup` never fetched a moved SDK.** When the workspace
   already exists, the recipe takes its "already present" branch. Issue 1279
   made that branch re-register the SDK with cmake, but only if the SDK
   *resolves*. A pin the store does not hold resolves to nothing, and the
   branch said nothing about it. So `runner-bootstrap.sh` (measured, log
   `runner-bootstrap-118.log`) re-provisioned `west`, reported every step OK,
   and still failed the doctor on the SDK. The only way through was `--force`,
   which re-creates the whole workspace.
2. **The loop stops on the commonest 78, and the stop is silent.** An SDK bump
   is a normal PR. On every host whose store predates it, the next container
   exits 78, and the loop stopped there without telling anyone. A loop that
   has stopped is not visible from GitHub: the job only looks queued.

## Fix

- `just zephyr setup`, "already present" branch: if `zephyr/SDK_VERSION` names
  an SDK the ladder cannot resolve, it now runs `nros setup --tool
  <zephyr-sdk tool>` before registering. That step is idempotent and keyed by
  store path, so it costs nothing when the SDK is present. If the index entry
  provisions a different version from the one the workspace asks for, it
  stops and names both versions. `--skip-sdk` still leaves the SDK alone.
- `runner-loop-container.sh`, exit 78: runs `runner-bootstrap.sh <labels>` once
  and retries. A second 78 straight after that re-provision still stops the
  loop, because restarting again is the storm that exit code exists to
  prevent. A finished job re-arms the one heal.

## Remaining

- **A stopped loop is still invisible.** The heal removes the pin-move cause.
  Any other 78, three run failures in a row, or a host that went away still
  leave jobs queued with no red. Wanted: a scheduled check that fails when a
  job for a self-hosted label has been queued longer than a lane's budget.
  Listing runners needs admin, but `actions:read` can see queued jobs and their
  labels, so a `GITHUB_TOKEN` can do it.
- The host-118 runner is restored by hand: the SDK is provisioned into its
  store with `nros setup --tool zephyr-sdk` inside the runner image, and the
  doctor must pass before the loop is restarted.
