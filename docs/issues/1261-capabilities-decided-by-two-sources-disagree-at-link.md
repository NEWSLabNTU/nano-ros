---
id: 1261
title: "An image's capabilities are decided in two places, and a disagreement
  surfaces as an undefined symbol at LINK -- native when SYSTEM is omitted,
  Zephyr always"
status: open
type: bug
area: build, zephyr
severity: medium
related: [issue-1260, issue-0745, issue-0353, phase-445]
---

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
