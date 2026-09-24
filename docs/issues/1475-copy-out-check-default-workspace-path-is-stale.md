---
id: 1475
title: "`check-copy-out.sh` defaults to `../nano-ros-workspace-4.4`, which `just
  zephyr setup` has not written for some time — so the nightly copy-out check fails
  on a path rather than on the copy-out it exists to test"
status: open
type: bug
area: ci, zephyr, testing
severity: medium
found: 2026-09-24
related: [1474]
---

## What happens

`nightly` run **35958890602** (schedule, 05:12), job **107502987723**
(`zephyr copy-out check (4.4)`), step `Copy-out build check`:

```
./scripts/zephyr/check-copy-out.sh "c/talker" "zenoh" "native_sim/native/64"
FAIL: Zephyr 4.4 workspace not set up at ../nano-ros-workspace-4.4
  run: NROS_ZEPHYR_VERSION=4.4 just zephyr setup
error: recipe `check-copy-out` failed on line 162 with exit code 1
```

The workspace **is** set up. The same job's earlier step provisioned it, and
its own log says where:

```
Workspace: /github/home/.nros/workspaces/zephyr/4.4
```

`scripts/zephyr/check-copy-out.sh:59` reads

```sh
WORKSPACE_DEFAULT="../nano-ros-workspace-4.4"
WORKSPACE="${NROS_ZEPHYR_WORKSPACE:-$WORKSPACE_DEFAULT}"
```

and nothing in `nightly.yml` sets `NROS_ZEPHYR_WORKSPACE`, so the default is
what runs. Setup writes workspaces under `$HOME/.nros/workspaces/zephyr/<line>`;
the sibling-directory layout the default names is a older convention.

## Why it matters

The job is named for the copy-out check and never performs one. Its advice
(`run: NROS_ZEPHYR_VERSION=4.4 just zephyr setup`) is also wrong for this
failure, because setup already ran and succeeded — a reader who follows it
gets a second workspace in the same place and the same message.

## What this is NOT

- **Not a setup failure.** Setup completed; the workspace and its SDK are
  present at the path quoted above.
- **Not issue 1474.** That is `cargo clippy` missing a component during a
  fixture build on a different lane. This job never reaches a build.
- **Not the `env.sh` heredoc defect** fixed alongside this filing. That one
  made setup emit a broken environment script while still exiting 0; it is
  upstream of nothing here, since `check-copy-out.sh` does not source `env.sh`.
  Both were found in this job, which is why they are easy to conflate.

## What would close it

Decide who states the workspace path, then make the two agree:

1. **The caller states it** — `nightly.yml` exports `NROS_ZEPHYR_WORKSPACE`
   for this step, from the same value the setup step used. Narrow, and leaves
   the stale default for the next caller to trip on.
2. **The script asks** — resolve the workspace the way setup does
   (`$HOME/.nros/workspaces/zephyr/<line>`, honouring the same overrides), so
   the default is derived rather than written down twice.

Acceptance is the job performing a copy-out build and reporting on it.
