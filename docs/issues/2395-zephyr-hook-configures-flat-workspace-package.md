---
id: 2395
title: "The Zephyr hook runs `nros configure` on a flat workspace's bringup package, which cannot see its siblings, so three west fixtures refuse at configure"
status: open
type: bug
severity: high
area: [zephyr, cmake, cli, ci]
related: [phase-530, 2390, 2189]
found: 2026-11-10
---

**Home: [phase-530](../roadmap/phase-530-nros-configure-single-pass.md)**. W4 made the Zephyr module hook call `nros configure` before Kconfig, and its project choice (Ruling R31) is what misses this layout.

## What

Tier-2 run 42697365095 (on main after #3396 and #3397) stopped 3 of the 5
`[[compile_check_fixture]]` west rows at configure, in
`cmake/NanoRosConfigureHook.cmake` (`nros configure ... failed (the Zephyr
module hook runs it before anything else)`):

- `west_bringup_zephyr` and `west_bringup_zephyr_cyclone_user_config`
  (`packages/testing/nros-tests/fixtures/multi_pkg_workspace_zephyr`, images
  `zephyr_native_sim` and `zephyr_native_sim_cyclonedds`):
  `2 <depend> name(s) resolve to nothing: listener_pkg, talker_pkg`.
- `zephyr_self_pkg_sibling` (`fixtures/zephyr_self_pkg/sibling`, image
  `zephyr`): `[image.zephyr] names entry 'caller', which is not a package in
  this workspace. Known packages: alpha_pkg`.

All three passed on the runner before W4 and have failed since.

## Why

All three rows are GENERATED applications: the west app (`zephyr_app/`,
`caller/`) states `-DNROS_IMAGE_SYSTEM_DIR=../<bringup>`. The hook then
configures the workspace that owns the bringup, which RFC-0085 D14's walk-up
finds as `<ws>/src/<bringup>`. These two fixtures are FLAT: the packages sit
beside each other at a root that a Cargo `[workspace]` declares, with no `src/`.
So the walk-up does not fire, and the project becomes the bringup directory
itself, which does not hold the packages it depends on or names as its entry.

Walking further up does not help: `nros configure` refuses the flat root
itself. Measured on copies of both fixtures:

```
Error: sync: no `src/<pkg>/package.xml` and no `package.xml` at root under .../multi_pkg_workspace_zephyr
  -- expected colcon-style workspace or single-pkg dir
```

Before W4 the hook ran no configure, so the layout never reached it.

## Fix

`_nros_image_kconfig_configure` (`zephyr/cmake/nros_image_kconfig.cmake`):
for a generated application whose bringup is not in a `<workspace>/src/`
directory, the hook does not run `nros configure`. It prints a STATUS line
naming the bringup and this issue, and the image's readers take the unseeded
path they took before W4. A standalone leaf (no `NROS_IMAGE_SYSTEM_DIR`) and a
generated application in a `src/` workspace are unchanged.

Measured locally (native_sim, Zephyr 3.7): `zephyr_self_pkg_sibling` refused
at configure before the change and builds after it.

Test: `tests/cmake-configure-hook-tests.sh` case O gains a flat arm: a
generated application whose bringup is not under `src/` makes no
`configure` call. It fails on the old hook (`configure <bringup>`, one call).

## Not done here

Seeding a flat workspace would need `nros configure` to take a flat layout.
These two fixtures are the only flat generated applications in the tree, and
`nros build` itself generates `src/`-layout workspaces, so that is left as is.

Acceptance: a tier-2 run on main builds all 5 `[[compile_check_fixture]]` west
rows.
