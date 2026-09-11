---
id: 1326
title: "Pointing a run at a private Zephyr workspace takes THREE environment
  variables under two different names, and nothing says so"
status: open
type: tech-debt
area: [testing, build, zephyr]
severity: low
found: 2026-09-11
related: [1166, 1171, 1010, phase-440, phase-448]
---

## What

Isolating a Zephyr run from the shared workspace — which issues 1166 and 1171
both needed, and phase-448 W2 needed again — requires all three of:

| variable | read by | for |
| --- | --- | --- |
| `NROS_ZEPHYR_WORKSPACE` | `scripts/lib/zephyr-workspace.sh` (the ONE resolver) | which workspace `west build` runs in |
| `NROS_ZEPHYR_BUILD_ROOT` | `just/zephyr-ci.just` AND `nros-tests`' `zephyr_build_root()` | where `build-<lang>-<case>-<rmw>` is written and read |
| **`ZEPHYR_NANO_ROS`** | `nros-tests`' `zephyr_workspace_path()` | whether `require_zephyr()` says the workspace EXISTS |

Miss the third and every Zephyr cell reports

```
Skipping test: Zephyr workspace not found
  Run: ./scripts/zephyr/setup.sh
```

— naming a setup script, on a host where the workspace is provisioned, the
fixtures are freshly built and `NROS_ZEPHYR_WORKSPACE` is exported. Under a bare
`cargo nextest` that is 9 FAILs with a message that points somewhere else
entirely.

## Already tracked, partially

`.config/zephyr-workspace-resolvers.txt` line 37 records the drift itself:

```
packages/testing/nros-tests/src/zephyr.rs  # Rust; keys on `ZEPHYR_NANO_ROS` and
accepts a `zephyr-workspace` SYMLINK, so it is a third rung set again
```

So the *duplicate resolver* is known backlog and this issue does not re-file it.
What is NOT recorded anywhere is the operational consequence above: that the two
resolvers key on DIFFERENT VARIABLE NAMES for the same fact, so exporting the
documented one is not enough, and the resulting failure blames the environment
for something the environment has.

## Fix direction

Cheapest honest fix, independent of retiring the duplicate: have
`zephyr_workspace_path()` accept `NROS_ZEPHYR_WORKSPACE` as its first rung
(keeping `ZEPHYR_NANO_ROS` for compatibility), so one variable answers one
question. The ratchet line can then shrink to "accepts a symlink" alone.

The full fix is the ratchet's: the Rust side calls the one resolver. That is
phase-440 W1's remaining work, not this issue's.
