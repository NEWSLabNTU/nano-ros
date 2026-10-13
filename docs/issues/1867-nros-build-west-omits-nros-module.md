---
id: 1867
title: "`nros build`'s west road never names the `nros` module, so every image built against the shared store workspace fails Kconfig"
status: open
type: bug
area: [cli, zephyr, build]
severity: high
found: 2026-10-13
related: [1258, 1379, 0196]
---

## Measured

The command below fails from any checkout that builds against the default store
workspace, `~/.nros/workspaces/zephyr/3.7`. That covers every checkout that does
not carry its own `zephyr-workspace/`.

```
nros build --workspace examples/workspaces/features zephyr_rust_params
```

It fails at Kconfig:

```
…/boards/native_sim_native_64/prj.conf:53: warning: attempt to assign the value 'y' to the undefined symbol NROS
…/extra_kconfig_options.conf:2: warning: attempt to assign the value 'y' to the undefined symbol NROS_CAPABILITY_LIFECYCLE
…
error: Aborting due to Kconfig warnings
```

`--dry-run` shows the command the CLI hands to west. It carries no
`ZEPHYR_EXTRA_MODULES`:

```
ZEPHYR_BASE=~/.nros/workspaces/zephyr/3.7/zephyr west build -b native_sim/native/64 -d … <entry> -- \
  -DAPPLICATION_CONFIG_DIR=… -DEXTRA_CONF_FILE=… -DNROS_RESOLVED_DIR=… -DNROS_BOARD_HEAP_BUDGET_BYTES=
```

The same command with `-DZEPHYR_EXTRA_MODULES=<checkout>` appended configures,
compiles and links against the same store workspace: 1314/1314 steps, rc 0.

## The workspace is not broken

The store workspace's manifest project, `agent-ab5c15194ed1fba8d/`, holds only
`west.yml`. That looks like a half-provisioned workspace, but it is the intended
shape. Since issue 1258 / phase-449 W1, the manifest project is unbound on
purpose:

- `scripts/zephyr/setup.sh` copies only the manifest file;
- `scripts/zephyr/unbind-manifest-project.sh` removes the old checkout symlink;
- the directory is named after the provisioning checkout's basename, which was
  an agent worktree here.

The design then requires every configuring `west build` to name its own checkout
with `-DZEPHYR_EXTRA_MODULES=<checkout>`. Issue 1379 gave that flag one spelling,
`scripts/lib/zephyr-module.sh`.

The CLI's west handoff (`packages/cli/nros-cli-core/src/cmd/build.rs`, the
`Driver::West` arm, `Handoff::new("west", …)`) is a builder that was never
taught to pass it. The colcon builder does pass it
(`colcon_nano_ros/task/nros/build.py`).

It stayed hidden because a checkout that still carries a pre-1258
`zephyr-workspace/` with a `nano-ros` symlink gets the module from west's
project list. The CLI's workspace search ranks that above the store.

## Why the gate missed it

`check-zephyr-module-binding` enforces exactly this rule. Its file set is shell
and `just` sources (`west build` text), so a `west` invocation built in Rust as
an argument vector is outside its reach. This is issue 0196's shape: the reach
is narrower than the rule. Issue 1379 already recorded the same class: phase-449
W1 fixed three of seven builders, and the gate was supposed to stop the next one.

## Fix

1. In the `Driver::West` arm, push `-DZEPHYR_EXTRA_MODULES=<nano_ros_root>` into
   the cmake zone. `nano_ros_root` is already resolved there, because the arm
   bails earlier without it. Pass the checkout ROOT, not `<root>/zephyr` (see
   `zephyr-module.sh`).
2. Widen `check-zephyr-module-binding` to the CLI's Rust `west` handoffs, so the
   flag cannot be dropped again. Write a selftest mutant first, as the gate's
   docstring asks.
3. Acceptance is a build: `nros build … zephyr_rust_params` against the store
   workspace links. Also confirm that a legacy bound workspace (symlinked
   manifest project) still configures, because
   `zephyr/cmake/nros_checkout_guard.cmake` arbitrates when both name a module.
