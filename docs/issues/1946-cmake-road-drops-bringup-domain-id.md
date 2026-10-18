---
id: 1946
title: "On the CMake road a C/C++ entry's session ignores the bringup's `domain_id`: `[system]` and `[image.<id>]` are both dropped, and only `-DNROS_DOMAIN_ID` reaches it"
status: open
type: bug
area: [build, cli, cmake]
severity: medium
found: 2026-10-18
related: [phase-441, 0794, 0161]
---

**Home: [phase-441](../roadmap/phase-441-rmw-on-target-verification.md)** — found building its W6 cell.

## What happens

`examples/workspaces/cpp/src/slirp_bringup/system.toml` stated `[system]
domain_id = 97`, then `[image.mps3_an536_slirp] domain_id = 54`. `nros build
slirp_bringup:mps3_an536_slirp` built the image both times with the session
on domain **0**:

- the generated `<image>_entry_nros_main_generated.cpp` bakes `.domain_id = 0`
  with no `NROS_BOOT_SET_DOMAIN` bit;
- `<image>_entry_nros_app_config_def.c` bakes `.domain_id = 0`.

No warning or refusal. On a DDS image the domain is the isolation axis, so a
domain the user wrote and the build dropped means a different bus.

## Where it is lost (traced, not yet fixed)

- The SystemModel's `execution.deploy.<fqn>.domain` is filled by
  ros-launch-manifest's `apply_to_launch` only when a `[host.*]` block covers
  the launch file (`system_config.rs:372-417`, falling back to `[system]
  domain_id` at 407-409). A bringup with no `[host.*]` gets no deploy entry,
  so `baked_session` (`nros-cli-core/src/codegen/entry/mod.rs:361`) answers
  `None`.
- `[image.<id>] domain_id` never feeds the deploy map; the nano-ros overlay
  reads it for placement only (`nros-orchestration-ir/src/overlay.rs:451`).
- Even with the boot-config bit set, the session would not read it on this
  road: `FreertosBoard::run_components` opens with `NROS_ENTRY_DOMAIN_ID`
  (`nros-cpp/include/rclcpp/detail/main.hpp:874`), which is defined only from
  a `-DNROS_DOMAIN_ID` cache variable (`cmake/NanoRosEntry.cmake:978-981`),
  and nothing calls `nros_boot_config_domain_id`.
- The configure line `nros build` emits (`cmd/build.rs:1261-1337`) never
  passes the image's domain; `NROS_ENTRY_APP_DOMAIN_ID` is hard-coded 0
  (`NanoRosEntry.cmake:1078`).

## Workaround in tree

phase-441 W6's fixture row states `NROS_DOMAIN_ID = "54"` in `cmake_defs`.

## What would close it

The image's resolved domain (`[image] domain_id`, else `[system]`) reaches
the C/C++ session on the CMake road through the same carrier the Rust and
Zephyr roads use for it, or the build refuses a stated domain it cannot
deliver. Acceptance is a build of `slirp_bringup:mps3_an536_slirp` with the
row's `NROS_DOMAIN_ID` removed whose session opens on 54, and the row
workaround deleted.
