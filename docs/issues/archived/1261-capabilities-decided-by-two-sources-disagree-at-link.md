---
id: 1261
title: "An image's capabilities are decided in two places, and a disagreement
  surfaces as an undefined symbol at LINK -- native when SYSTEM is omitted,
  Zephyr always"
status: resolved
type: bug
area: build, zephyr
severity: medium
related: [issue-1260, issue-0745, issue-0353, phase-445]
resolved: 2026-10-11
---

## 2026-10-11 — RESOLVED: the west road reads the image too

**Acceptance 2: MET.** The Zephyr module now asks the image before its first
read of `NANO_ROS_FEATURES` (`nros_resolve_image_features`,
`cmake/NanoRosImageAgreement.cmake`). It runs the query the module already ran
for the RMW cross-check, `nros image-facts --for-entry <app> --cmake`, which
now also emits `NROS_IMAGE_FEATURES`: the declared axes, through the same
`declared_capabilities` accessor the generated west application uses. The
query runs once per configure and both readers share its answer.

- `NANO_ROS_FEATURES` unset → it becomes the image's axes.
- Set, but missing a declared axis → configure stops, naming the missing axes.
- Set to a superset → left alone.
- An application in no workspace → unchanged, so `-DNANO_ROS_FEATURES` is
  still its only source.

Measured on `examples/workspaces/features` `[image.zephyr_cpp_params]`
(native_sim/native/64, store Zephyr 3.7). The generated application was copied
out with its `set(NANO_ROS_FEATURES …)` and digest lines deleted, which is a
hand-written app that states nothing:

- old module: `west rc=2`, `undefined reference to
  nros_cpp_register_parameter_services` and `… nros_cpp_lifecycle_autostart`;
- new module: `capability axes of image demo_bringup:zephyr_cpp_params:
  param_services, lifecycle (from its system.toml)`, `west rc=0`;
- the generated application, unchanged: `west rc=0`.

Gate: `just check workspace-system-required` gained cases D–G, which drive the
macro in script mode against a synced synthetic workspace. With the default
removed, D fails; with the refusal turned into a STATUS, E fails.

## 2026-10-11 — the workspace half refuses at configure (still OPEN for Zephyr)

**Acceptance 1: MET.** `nano_ros_workspace()` with no `SYSTEM` now asks the
same SSoT accessor (`nros config show --format cmake`, with no `--system`, so
it answers for the default or only bringup). If that bringup declares any
capability, configure stops naming the bringup and the argument:
`Pass SYSTEM demo_bringup to nano_ros_workspace() (issue 1261)`. A
bringup-less workspace, which is valid and is what `nros build` generates in
package mode, is untouched. So is a host with no CLI: the lookup is OPTIONAL
there, and with no CLI there is no codegen to disagree with.
Gate: `just check workspace-system-required`
(`tests/cmake-workspace-system-required-tests.sh`) configures three synthetic
workspaces:
- A: capability declared, SYSTEM omitted → refused;
- B: SYSTEM given → not refused;
- C: no bringup → configures.

With the refusal disabled, A fails both of its assertions. Re-measured before
the change, the phase-445 re-measure got
`undefined reference to nros_cpp_register_parameter_services` at link.

**Acceptance 2: partly.** A Zephyr image built by `nros build` gets
`NANO_ROS_FEATURES` from the generated west application (e0b8504beb, #1681).
A HAND-WRITTEN west application under a bare `west build` still needs
`-DNANO_ROS_FEATURES`, and `zephyr/CMakeLists.txt`'s comment about resolving
from the entry's BRINGUP still stands. Issue 1288 is retiring the in-tree
hand-written applications. The road itself stays open here.


## Symptom

A downstream (Autoware Safety Island) declared `param_services` in its
bringup's `system.toml`, which every component that calls `declare_parameter`
needs since phase-426 W4. The native build then failed at the final link:

```
/usr/bin/ld: native_entry_nros_main_generated.cpp:(.text+0x3bc):
    undefined reference to `nros_cpp_register_parameter_services'
```

Nothing at configure time named the cause. The generated entry had been told
the image has parameter services, and the library it links had not.

## Cause

Two consumers read the capability, from two different routes:

- **codegen** reads the bringup's `system.toml` through `nros sync`, so the
  generated entry calls `nros_cpp_register_parameter_services`;
- **the library feature set** comes from `NANO_ROS_FEATURES`, which
  `nano_ros_workspace()` fills only when it is given `SYSTEM`
  (`cmake/NanoRosWorkspace.cmake`, the `if(_NRW_SYSTEM)` block that runs
  `nros config show --format cmake`). The downstream's call passed `BACKEND`,
  `PLATFORM` and `SUBDIRS` and no `SYSTEM`, so nros-cpp was built without
  `param-services`.

Before phase-323, posix got `param_services` unconditionally, so omitting
`SYSTEM` cost nothing and the two routes could not disagree on the one
platform most people build first. Phase-323 made capabilities come from the
declaration, which is right, and left the omission silent.

**The Zephyr lane has no such route at all.** `zephyr/CMakeLists.txt` reads
`NANO_ROS_FEATURES` from the caller and says so:

```
# Issue 0745 -- the Zephyr lane has no workspace-style capability
# resolution (`nros config show`) yet; the consumer passes
# -DNANO_ROS_FEATURES=<axes> ... Follow-up: resolve from the entry's
# BRINGUP like nano_ros_workspace(SYSTEM ...) does.
```

Issue 0745 is archived, so that follow-up lives only in the comment. Every
Zephyr consumer that declares a capability must restate it on the `west build`
line, and one that does not gets the same link failure. The entry already
names its bringup (`nano_ros_add_executable(... BRINGUP <pkg>)`), so the
information is present in the build; it is just not the source.

The downstream now passes `nros config show --format cmake` to `west build`
with `-C` (its output is a `set(... CACHE ... FORCE)` line, which is exactly
an initial-cache file), so the list is still written once. That is a
workaround each consumer has to discover.

## Fix shape

One source per image, and a disagreement refused where it can be named:

- Resolve the capability axes from the entry's `BRINGUP` in the Zephyr lane,
  the way `nano_ros_workspace(SYSTEM ...)` does on the workspace path.
- Where a generated entry needs a capability, have the entry assert that the
  library it links was built with it -- at configure time, naming the missing
  `SYSTEM` argument or `-DNANO_ROS_FEATURES` entry -- instead of letting the
  linker report a symbol.
- Or make `SYSTEM` required whenever a bringup declares any capability, since
  the bringup is already known to `nros sync`.

## Acceptance

- A workspace whose bringup declares `param_services` and whose
  `nano_ros_workspace()` omits `SYSTEM` fails at CONFIGURE with a message
  naming `SYSTEM`, or builds correctly because the capability was resolved.
- A Zephyr image declaring `param_services` in its bringup builds with no
  `-DNANO_ROS_FEATURES` on the command line.
