---
id: 351
title: "A declared `param_services` / `lifecycle` never reaches the C/C++ cargo feature list — the posix always-on IS the lowering, and it masks the missing wiring"
status: open
type: bug
area: build
related: [phase-315, issue-0311, phase-314, rfc-0004]
---

## Finding (2026-07-31)

`examples/workspaces/ws-params-cpp` declares the capability:

```toml
# src/demo_bringup/system.toml
[param_services]
enabled = true
```

and the bake lowers it correctly:

```cmake
# <bake>/system_config.cmake
set(NANO_ROS_FEATURES "param_services" CACHE STRING "nano-ros capability axes" FORCE)
```

But **nothing includes that file on the workspace path**, and the workspace's
own cache reads:

```
NANO_ROS_FEATURES:STRING=
```

`nros-c` / `nros-cpp` build their capability list from exactly that variable
(`set(_caps ${NANO_ROS_FEATURES})`), so the declaration contributes nothing. The
crates are compiled with

```
--features=ros-humble,rmw-zenoh-cffi,std,platform-posix
```

— no `param-services`, no `lifecycle-services`.

## Why nobody noticed

Both `nros-c/CMakeLists.txt` and `nros-cpp/CMakeLists.txt` (and the umbrella)
carry:

```cmake
# Hosted keeps param/lifecycle always-on (alloc-gated); embedded opts in.
if(NANO_ROS_PLATFORM STREQUAL "posix")
    list(APPEND _caps param_services lifecycle)
endif()
```

That is not a convenience — on posix it is **the only path** by which those two
capabilities ever reach the cargo feature list. `NanoRosCapabilities.cmake` says
so in its own comments, without drawing the conclusion:

```cmake
elseif(_feat STREQUAL "param_services")
    # Known axis, no CMake knob (entry-umbrella-only; the `#define`
    # NROS_SYSTEM_PARAM_SERVICES in system_config.h is its only C/C++ lowering).
elseif(_feat STREQUAL "lifecycle")
    # lifecycle-services is always compiled into nros-cpp/nros-c
    # (CMakeLists.txt always-on features)
```

So the declaration path and the working path are disjoint. Everything builds
because the always-on covers every posix consumer regardless of what it
declared, which also means an example that FORGOT to declare is
indistinguishable from one that did.

## How it was found

phase-315 W4 proposed removing the posix always-on to make hosted consistent
with embedded ("capabilities only when declared"). The removal was checked
first, and the check said it was safe: the only non-comment callers of the gated
API are `ws-params-c` and `ws-params-cpp`, and **both declare**. Lifecycle has no
direct callers at all — its services are registered by generated entry code that
codegen emits only when declared.

Removing it then broke the declaring workspace:

```
ParamTalker.cpp:(.text+0x44): undefined reference to `nros_cpp_get_param_integer'
```

which is the opposite of what "both declare" predicted, and is what exposed the
disjoint paths. The change was reverted; main is unaffected.

## Consequence

`system.toml` is not the SSoT for capabilities on the C/C++ side, only on the
Rust side (phase-315's facade). Concretely:

* an embedded C/C++ image gets a capability only if declared — correct;
* a posix C/C++ image gets `param_services` + `lifecycle` whether declared or
  not, and CANNOT get them by declaring alone;
* so the same `system.toml` produces different capability sets by platform, and
  the hosted side is unable to fail when the declaration is missing.

This is the same shape as issue 0311 / phase-314 — one axis, two sources that
cannot disagree because only one is consulted.

## Fix sketch

The lowering has to exist before the always-on can go. Options, in rough order
of preference:

1. **Include the bake's `system_config.cmake` before `add_subdirectory`** on the
   workspace path, so `NANO_ROS_FEATURES` is populated where `nros-c`/`nros-cpp`
   read it. Smallest change; needs the bake dir known at configure time.
2. **Resolve capabilities through the CLI at configure time** — the workspace
   cmake already shells out to `nros`, and `capability_enabled()` is the SSoT
   accessor (it honours both the typed blocks and `[system].features`).
3. **Give `param_services` / `lifecycle` real `cmake_token`s** in
   `NanoRosCapabilities.cmake`, matching `safety` → `NANO_ROS_SAFETY_E2E`. Most
   uniform, but the Rust `Capability` registry is the SSoT for
   `(declared, cmake_token)` and a drift test asserts they match, so the row
   changes there too.

Only after one of those lands can the posix always-on be removed — and at that
point it must be, or the two paths drift again.

Acceptance for the eventual fix: `ws-params-cpp` compiles `nros-cpp` **with**
`param-services` traceable to its declaration, and a posix workspace that
declares nothing compiles **without** it.
