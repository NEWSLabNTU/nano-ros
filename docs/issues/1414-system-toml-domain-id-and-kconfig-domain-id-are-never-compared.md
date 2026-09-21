---
id: 1414
title: "system.toml's domain_id is baked as NROS_SYSTEM_DOMAIN_ID and read by
  nothing; the Zephyr image takes CONFIG_NROS_DOMAIN_ID, and no layer compares
  the two"
status: open
type: bug
area: [cli, zephyr, cmake]
severity: medium
found: 2026-09-21
related: [issue-0794, issue-0934, phase-460, rfc-0049]
---

## What was observed

Brief B (2026-09-18) recorded the Autoware Safety Island with
`src/safety_island_bringup/system.toml` saying `domain_id = 2` while
`src/zephyr_entry/prj-cyclonedds.conf` baked `CONFIG_NROS_CYCLONE_DOMAIN_ID=10`
and the board `.config` carried `CONFIG_NROS_DOMAIN_ID=10`. Every image built
on 10; every document derived from `system.toml` said 2. By 2026-09-21 the
island's two `system.toml` files both say 10 - corrected by a person reading
the brief, which is the only check that ran.

## The two roads (verified at 783cdfa14)

* `codegen-system` resolves the domain through the `[deploy.<target>]` /
  `[system]` ladder and writes
  `#define NROS_SYSTEM_DOMAIN_ID <n>u` into `system_config.h`
  (`packages/cli/nros-cli-core/src/cmd/codegen_system.rs:856-858`).
  `grep -rln NROS_SYSTEM_DOMAIN_ID zephyr cmake packages/api packages/boards
  scripts` is empty: the define is compiled into every image and read by no
  source file.
* The image's domain is `CONFIG_NROS_DOMAIN_ID` (`zephyr/Kconfig:1652`,
  default 0) via `packages/api/nros-c/include/nros/zephyr/app_config.h:90`
  into `RmwConfig`; Cyclone's `CONFIG_NROS_CYCLONE_DOMAIN_ID` defaults to it
  (`Kconfig:209-212`). The C++ entry's compile-time `NROS_ENTRY_DOMAIN_ID`
  comes from the cmake variable `NROS_DOMAIN_ID` when a caller sets it
  (`cmake/NanoRosEntry.cmake:684-688`), not from the bake.

Two declarations, two readers, no comparison. Issue 0934's redundancy map
lists the domain among the duplicated knobs; this issue is the concrete
disagreement it predicted, measured.

## What would fix it

phase-460 W4. `zephyr/cmake/nros_system_generate.cmake`, after the bake it
runs, compares `NROS_SYSTEM_DOMAIN_ID` against `CONFIG_NROS_DOMAIN_ID` and,
when set, `CONFIG_NROS_CYCLONE_DOMAIN_ID`, and refuses on disagreement naming
all three. Precedence does not change: Kconfig remains what the image bakes
(RFC-0049); the check refuses the silent case only. Whether `system.toml`
should become the single writer is RFC-0049's question and is not decided
here.

## Acceptance

A fixture `.config` with `CONFIG_NROS_DOMAIN_ID=2` against a bringup declaring
10 fails the configure naming both; equal values pass.
