---
id: 1998
title: "`nros build` hands west no `-Dnano_ros_ROOT`, so a Zephyr workspace image cannot find its own package config"
status: resolved
resolved: 2026-10-22
resolved_in: "PR #2480"
type: bug
area: cli, build, zephyr
severity: high
found: 2026-10-22
related: [1304, 1867, 1258, phase-439, rfc-0103]
---

**Home: [phase-439](../../roadmap/phase-439-resolve-before-configure.md)** — found while re-measuring its configure-pass count (W5).

## What happens

`nros build zephyr` from `examples/workspaces/cpp`, on `main` (`4b4f372522`),
in a shell where `activate.sh` was sourced, with a Zephyr 3.7 workspace:

```
CMake Error at CMakeLists.txt:46 (find_package):
  By not providing "Findnano_ros.cmake" in CMAKE_MODULE_PATH this project has
  asked CMake to find a package configuration file provided by "nano_ros",
  but CMake did not find one.
```

The generated west app (`build/zephyr-zenoh/zephyr_entry/CMakeLists.txt`)
opens with `find_package(nano_ros REQUIRED)`. `nano_rosConfig.cmake` sits at
the checkout root and is found through `nano_ros_ROOT`.

## Why

Issue 1304 hit this on the cmake roads after `activate.sh` stopped being the
only thing that could supply the root, and fixed it there: `nros build` pushes
`-Dnano_ros_ROOT=<root>` on the workspace lane, the single-package leaf and the
cmake member. RFC-0103 D6 (amended 2026-10-12) then took the export out of
`activate.sh` entirely, so the flag is now the only way the root arrives. The
west arm was never given it. Issue 1867 added `-DZEPHYR_EXTRA_MODULES` to the
same line and stopped there.

So every `nros build` of a Zephyr workspace image, which goes through a
generated app that calls `find_package(nano_ros)`, failed at configure.
Passing `-- -Dnano_ros_ROOT=<checkout>` by hand gets past it, which is how it
was confirmed.

## Fix

One spelling, `crate::builder::nano_ros_root_arg`, called from all four roads,
the west arm included (in the cmake zone after `--`, before the caller's own
arguments, so a caller's `-Dnano_ros_ROOT` still wins).

Sweep: `git grep -n 'Dnano_ros_ROOT={}' -- 'packages/cli/**/*.rs'` finds only
the helper.

Test: `build_verb_pipeline::a_west_image_names_the_nano_ros_root_its_app_imports`.
It fails with the west push removed (`no -Dnano_ros_ROOT after --`) and passes
with it.
