---
id: 351
title: "No declared capability reaches the C/C++ build on the workspace path — one broken mechanism, three different masks (posix always-on, per-fixture `-D`, and a closed issue cited as cover)"
status: open
type: bug
area: build
related: [phase-315, issue-0311, phase-314, rfc-0004, issue-0118]
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

## All three axes share the mechanism — and all three are broken

`safety`, `param_services` and `lifecycle` take the SAME path:
`NANO_ROS_FEATURES` → `set(_caps ${NANO_ROS_FEATURES})` in nros-c/nros-cpp →
`nros_feature_set(... CAPABILITIES ${_caps})` → cargo feature. The registry
differs only in whether an axis also owns a cmake `option()`:

| declared | nros_feature | c_define | cmake_token |
| --- | --- | --- | --- |
| `safety` | `safety-e2e` | `NROS_SYSTEM_SAFETY_E2E` | `NANO_ROS_SAFETY_E2E` |
| `param_services` | `param-services` | `NROS_SYSTEM_PARAM_SERVICES` | — |
| `lifecycle` | `lifecycle-services` | `NROS_SYSTEM_LIFECYCLE` | — |

Since `NANO_ROS_FEATURES` is empty on the workspace path, none of the three
arrives. They differ only in how that is MASKED, which is why it reads as three
unrelated quirks instead of one bug:

* **`param_services` / `lifecycle`** — masked implicitly by the posix always-on.
* **`safety`** — masked explicitly, per fixture, in `examples/fixtures.toml`:

  ```toml
  cmake_defs = { NANO_ROS_SAFETY_E2E = "ON" }
  ```

  with the manifest saying so plainly: *"`[system].features = ["safety"]` in
  system.toml declares the safety axis, but the cmake feature-lowering
  (`nros_lower_system_features` → `NANO_ROS_SAFETY_E2E`) is not yet wired into
  the per-entry `nano_ros_entry` build (issue #118). Pass the knob directly."*

**That citation is stale.** Issue 0118 is `status: resolved` (phase-269) and was
about the C/C++ executor-component integrity READBACK API, not about cmake
lowering. The API landed; the wiring gap it was cited for never had an issue of
its own. So the workaround now points at a closed ticket, which is how a
temporary `-D` becomes permanent.

A standalone example is not affected, and shows what the workspace path lacks —
it sets the variable itself before pulling nano-ros in:

```cmake
# examples/native/{c,cpp}/safety-listener/CMakeLists.txt
set(NANO_ROS_FEATURES "safety")
```

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
3. ~~Give `param_services` / `lifecycle` real `cmake_token`s.~~ **Rejected on
   inspection:** a `cmake_token` exists to flip a cmake `option()`, which
   `safety` needs because it gates C/C++ CODE. These two need only a cargo
   feature, and `nros_feature_set` already maps them (`param_services` →
   `param-services`, `lifecycle` → `lifecycle-services`). Adding tokens changes
   nothing while `NANO_ROS_FEATURES` is empty, and is redundant once it is not.

Whichever lands must fix all THREE axes, and the safety fixtures' explicit
`cmake_defs` should be deleted in the same change — otherwise the mask outlives
the bug again, exactly as it did behind a resolved #118.

Only after one of those lands can the posix always-on be removed — and at that
point it must be, or the two paths drift again.

Acceptance for the eventual fix: `ws-params-cpp` compiles `nros-cpp` **with**
`param-services` traceable to its declaration, and a posix workspace that
declares nothing compiles **without** it.
