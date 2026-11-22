---
id: 2862
title: "A bringup nested below src/ is placed at the wrong workspace root, and an owned C/C++ west entry's census is taken but never sizes the image"
status: open
type: bug
severity: medium
area: [cli, zephyr, sizing]
related: [issue-2433, issue-2395, issue-2405, issue-2482, rfc-0071]
found: 2026-11-23
---
Found by a downstream safety-island port, whose test bringups live at
`<ws>/src/test_bringup/<name>/`; seen on main 545ae12ec1.

## What

Two faults on one road; the first hides the second.

1. **The workspace root of a nested bringup.** Every rule that derives a
   place from a bringup takes `<ws>` as the bringup's GRANDPARENT
   (`<ws>/src/<bringup>`): `model_location::generated_launch_root` and the
   build-output rung of `model_search_paths`
   (`packages/core/nros-orchestration-ir/src/model_location.rs`). For
   `<ws>/src/test_bringup/unit_test` that is `<ws>/src`, so the launch and
   model are looked for under `<ws>/src/build/nros/...`. The Zephyr module
   hook asks the same question its own way, "is the parent named `src`"
   (`zephyr/cmake/nros_image_kconfig.cmake`, `_nros_image_kconfig_configure`),
   and for a nested bringup skips `nros configure`:
   `nros configure not run ... is not in a <workspace>/src/ directory, so no
   workspace configure can take it; its readers are unseeded`.
   colcon finds packages at any depth under `src/`, so the layout is legal.
2. **An owned C/C++ west entry's census.** `nros build` takes the census of
   the owned entry (`entity-census take ... -> build/nros/census/owned/...`),
   but on the west road nothing consumes it: the stage-3.5 resolve reports
   `entity counts not derived; configured defaults kept`, the message-bound
   seed reports `message bounds not seeded` (it looks for a model an owned
   entry with no launch cannot have), and the configure never calls
   `nros_record_owned_entity_facts`, which the cmake road's generated root
   calls for the same image on FreeRTOS (issue 2433). With the bringup at
   `<ws>/src/<bringup>` the hook DOES run `nros configure`, and the configure
   then fails its promise check:
   `the module named a sizing descriptor (.../nros/sizing/west-cargo.toml)
   that no entry of this configure wrote`.
   So fixing (1) alone turns the nested image's silent defaults into a build
   failure; the two land together.

## Repro

A workspace with `src/c_talker` (`examples/threadx-linux/c/talker`'s
package), `src/zephyr_c_talker` (a west app: `find_package(Zephyr)`,
`find_package(nano_ros)`, `nano_ros_add_executable(c_talker
../c_talker/src/main.c BOARD zephyr DEPLOY zephyr)`), and a bringup with
`[image.zephyr] board = "zephyr"  entry = "zephyr_c_talker"` and no
`[[component]]` row.

- Bringup at `src/test_bringup/talker_bringup`: `nros build
  talker_bringup:zephyr` exits 0; census taken (1 node, 1 pub, 1 timer);
  `.../src/build/nros/launch/talker_bringup/launch/system.launch.xml does not
  exist`; `nros configure not run`; every zenoh pool left at its literal
  default.
- Bringup at `src/talker_bringup`: census taken, `nros configure` runs, the
  configure stops on the sizing-descriptor promise above (exit 1).
