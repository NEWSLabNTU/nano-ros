---
id: 1880
title: "In a linked worktree, cmake and the setup/fixture scripts run the PARENT checkout's `nros` from PATH"
status: resolved
resolved: 2026-10-14
type: bug
area: build
severity: medium
found: 2026-10-13
related: [1280, 1850, RFC-0103, phase-484]
---

## Measured

In a fresh linked worktree that had not sourced its own `activate.sh`,
configuring `examples/native/c/talker` failed:

```
nano-ros: …/examples/native/c/talker/system.toml:
error: unexpected argument '--image-env-out' found
```

PATH named the main checkout's `packages/cli/target/release/nros`, an older
binary, and cmake ran it. With `NROS_CLI` pointing at the worktree's binary,
the configure got one step further. Then the bootstrap's `tools/setup.sh`
picked the same parent binary, and the stale-binary guard stopped it:

```
Error: this `nros` does not belong to the checkout it is being run against.
    running: /home/aeon/repos/nano-ros/packages/cli/target/release/nros
    checkout: /home/aeon/repos/nano-ros/.claude/worktrees/wver
```

## Cause

The shell resolver `nros_cli_bin` (`scripts/build/cargo.sh`) has asked "this
checkout's build" before PATH since phase-484 W2d. Issue 1850 routed literal
paths through it. But several lookups asked PATH alone and never reached
it:

- cmake's `nros_resolve_cli` (`cmake/NanoRosCli.cmake`): `$NROS_CLI` → cache →
  `find_program` (PATH, then the store), with no checkout rung;
- `tools/setup.sh`'s own `resolve_nros_bin`: PATH first, `$NROS_CLI` last;
- private `find_program(nros)` calls in `cmake/toolchain/riscv64-threadx.cmake`
  and `zephyr/cmake/nros_rmw_cyclonedds.cmake`;
- `command -v nros` in `compile-check-fixtures.sh` (6 sites), `jobserver-pool.sh`
  (2), `zephyr-fixture-leaves.sh`, `nros_seed_leaf_lock`,
  `check-tier-preconditions.sh`, `check-entity-census.sh`, `lib/zephyr-sdk.sh`,
  `zephyr/resolve-fvp-bin.sh` and `zephyr/setup.sh`.

This is issue 1280's class: a linked worktree inherits its parent shell's PATH.
`check-host-tool-path-literals` caught a spelled path, but not a lookup that
asks PATH.

## Fix

- `nros_resolve_cli` gains rung 2: the CLI built by the checkout the cmake
  modules live in, ahead of the cache rung, since a build dir configured before
  the fix cached the PATH answer. It mirrors `nros_cli_bin`. An installed SDK
  root has no `packages/cli/target`, so the rung is silent there.
- Every lookup listed above now asks `nros_cli_bin` (shell) or `nros_resolve_cli`
  (cmake). `tools/setup.sh`'s private ladder is gone.
- `check-host-tool-path-literals` refuses a PATH-only lookup of `nros`
  (`command -v nros`, `which nros`, cmake `find_program(<v> nros)`) in shell,
  `just` and cmake code, and its scope grows to `tools/`, `cmake/`, `zephyr/` and
  `integrations/`. Sites that ask PATH on purpose carry a `host-tool-path-ok:`
  marker. Those are the resolvers' own PATH rungs, the doctors and shadow checks,
  the "did you source activate.sh" preconditions, the clean-container probes,
  the out-of-tree PX4 template and the `simple-workspace` test project.

## Verified

- Before: the configure above failed with `unexpected argument`, in worktree
  `wver` at `origin/main` e98d30f977.
- After: worktree `wcli`, with PATH still naming the main checkout's `nros`
  first and no `activate.sh`, configures and builds `c_talker`. Its cache records
  `_NANO_ROS_CODEGEN_TOOL=…/worktrees/wcli/packages/cli/target/release/nros`.
- Negative control: the gate's matcher flags 23 PATH-only lookups across the 13
  routed files at `origin/main`, and 0 after. The self-test has 26 cases (8 new).
