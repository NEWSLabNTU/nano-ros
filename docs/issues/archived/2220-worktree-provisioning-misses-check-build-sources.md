---
id: 2220
title: "`setup-worktree` does not provision the board sources `check::build` compiles, so a fresh worktree's gate depends on what the shared store happens to hold"
status: resolved
type: bug
severity: medium
area: [build, ci]
related: [1513, 2207]
found: 2026-11-02
---

## What

`.config/worktree-provisioning.txt` is the one list `just setup-worktree`
provisions and `_require-worktree-provisioning` checks before `just ci gate`
starts (issue 1513). It named five sources: zenoh-pico, micro-cdr,
micro-xrce-dds-client, cyclonedds and play_launch.

`check::build`'s `workspace-all` and `test-targets` gates also compile
`nros-board-freertos` and `nros-board-threadx`, whose build scripts locate four
more store-first sources through `nros_build_paths::source`:

| Source            | Read by                         |
| ----------------- | ------------------------------- |
| `freertos-kernel` | `nros-board-freertos` build.rs  |
| `lwip`            | `nros-board-freertos` build.rs  |
| `threadx`         | `nros-board-threadx` build.rs   |
| `threadx-netxduo` | `nros-board-threadx` build.rs   |

None was on the list. A worktree's gate passed only while the shared store
(`$NROS_HOME/sources`) already held each at the index pin, left there by some
other build. When a pin moved (lwip to `96b63e31`, netxduo to `393b8625`) every
fresh worktree's gate went red. A checked-out submodule at its pin does not help:
for a store-first source the checkout counts only as a local edit, i.e. when it
is OFF the pin.

## Measured

Fresh worktree from `origin/main`, `just setup-worktree`, `just setup-cli`,
`just check workspace-all`:

```
lwip: not provisioned anywhere this build looks:
  ...
  store      ~/.nros/sources/lwip/2.2.0+96b63e31 absent
threadx-netxduo: not provisioned anywhere this build looks:
  ...
  store      ~/.nros/sources/threadx-netxduo/6.4.x+393b8625 absent
```

The store held `lwip/2.2.0+6233eb61` and `threadx-netxduo/6.4.x+d0c2e80e`, the
previous pins. `freertos-kernel` and `threadx` were present at their pins, which
is the same luck, not provisioning.

## Resolution

The four sources are rows of `.config/worktree-provisioning.txt`, each naming
its reader. `setup-worktree` already provisions a listed path with a store row
through `nros setup --source <row>`, so no code changed: the list was the
missing fact, the same shape as issue 1513.
