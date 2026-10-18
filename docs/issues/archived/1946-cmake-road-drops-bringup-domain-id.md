---
id: 1946
title: "On the CMake road a C/C++ entry's session ignores the bringup's `domain_id`: `[system]` and `[image.<id>]` are both dropped, and only `-DNROS_DOMAIN_ID` reaches it"
status: resolved
type: bug
area: [build, cli, cmake]
severity: medium
found: 2026-10-18
resolved: 2026-10-18
related: [phase-441, 0794, 0161]
---

**Home: [phase-441](../../roadmap/phase-441-rmw-on-target-verification.md)** — found building its W6 cell.

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

## RESOLVED 2026-10-18

**Carrier.** The image's domain now reaches a generated C/C++ entry through
the resolver the cargo and west roads already read,
`nros_orchestration_ir::leaf_system::read_image` (`[image.<id>] domain_id`,
then the image defaults, then `[system] domain_id`). `nros build` writes it
into each generated `nano_ros_add_executable(...)` as `DOMAIN_ID <n>`
(`builder/cmake_root.rs`, `CmakeEntry::domain_id`), PER ENTRY, because several
images share one configure and a configure-wide `-D` cannot say which image
it is for. Both CMake frames parse it (`NanoRosVerbs.cmake` forwards it,
`NanoRosEntry.cmake` consumes it), and `check-generated-cmake-keywords` refuses
an emitted keyword either frame does not parse; measured: with the verb's half
removed, the keyword fell into `SOURCES` and the configure failed in
`nros codegen source-language`, and the gate failed too.

`NanoRosEntry.cmake` bakes it as `NROS_ENTRY_DOMAIN_ID` (what the C and C++
sessions open with) and as the FreeRTOS app config's `.domain_id`, which was
a literal 0.

**Two producers, no ranking.** A `-DNROS_DOMAIN_ID` is still honoured where
the image states nothing (hand-written entries, standalone leaves), but when
both are present and disagree the configure FAILS naming both, instead of
choosing one silently.

**Acceptance, as stated above.** `slirp_bringup:mps3_an536_slirp` with the
row's `NROS_DOMAIN_ID` removed and the cached value cleared
(`cmake -U NROS_DOMAIN_ID`): the configure emits `DOMAIN_ID 54`, the entry
compiles with `NROS_ENTRY_DOMAIN_ID=54`, the app config bakes
`.domain_id = 54`, and `just freertos test-ros2-cyclone` passes 3/3 against a
ROS 2 peer on domain 54. The row workaround is deleted and the domain is
stated once, as the image's `domain_id`. A mismatching `-DNROS_DOMAIN_ID=7`
was measured to stop the configure with the message above.

**Not covered: standalone CMake leaves.** A leaf `system.toml` beside a
hand-written `CMakeLists.txt` is read by `nano_ros_read_leaf_system`, whose
`NROS_LEAF_DOMAIN_ID` still feeds only the Zephyr agreement check. The fixture
lanes that bake Cyclone domains into such leaves do it with `-DNROS_DOMAIN_ID`
(`just/threadx-linux.just` 107/108, `just/threadx-riscv64.just` 128/129,
`just/freertos.just` 60) while those leaves state `domain_id = 0`, so wiring
the leaf's domain in would make every one of them refuse. Closing that half means
moving each lane's domain into its leaf `system.toml` first.
