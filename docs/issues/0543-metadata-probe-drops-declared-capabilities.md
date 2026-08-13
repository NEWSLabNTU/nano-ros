---
id: 543
title: "The metadata probe builds components without the bringup's declared capabilities, so a component using a capability-gated API cannot be probed"
status: open
type: bug
area: build
related: [issue-0542, issue-0522, phase-313, phase-314]
---

## Symptom

`nros sync examples/workspaces/safety` cannot produce sidecars for the two
components that use the safety API:

```console
sync: source metadata — no producer for cpp_safety_listener_pkg::cpp_safe_listener
  (metadata probe build failed (exit 2):
   error: ‘class nros::Node’ has no member named ‘create_subscription_with_safety’;
          did you mean ‘create_subscription_with_info’?)
```

Found while fixing issue 0542, which was the OTHER reason this workspace could
not probe. With 0542 fixed the failure count goes 3 → 2, and what remains is
this.

## It is not API drift — the method exists, gated

`packages/api/nros-cpp/include/nros/node.hpp`:

```cpp
#if defined(NANO_ROS_SAFETY_E2E)
    ...
    /// Requires `NANO_ROS_SAFETY_E2E=ON` (lowered from
    /// `[system].features = ["safety"]` via `NanoRosCapabilities.cmake`).
    Result create_subscription_with_safety(...);
#endif // NANO_ROS_SAFETY_E2E
```

The workspace declares the capability
(`examples/workspaces/safety/src/demo_bringup/system.toml`):

```toml
features = ["safety"]
```

and `cmake/NanoRosCapabilities.cmake` lowers it: `safety → NANO_ROS_SAFETY_E2E`.

The chain is intact for a real build. What breaks is the PROBE: the CMakeLists
`metadata_probe_cmake.rs` generates carries no capability input at all — `git
grep -n 'capabilit\|NANO_ROS_SAFETY' packages/cli/nros-cli-core/src/orchestration/
metadata_probe_cmake.rs` returns nothing. So the probe compiles the component's
own sources against a header where the declared API is `#if`'d out.

## Why this shape keeps recurring here

The probe assembles a feature set for a component WITHOUT the thing that decided
that component's feature set — the bringup. Issue 0542 was the neighbouring
mistake in the same assembly (a hook applied to the wrong crate), and phase-314
exists because the same list used to be computed in three places that disagreed.

The general statement: a probe that compiles the user's source must build it with
the user's configuration, or it is answering a question about a different
program. Anything gated behind a capability, an RMW choice or a ROS edition can
present as "your code does not compile" when the code is fine.

## Direction

`nros sync` already knows the bringup — it resolves `system.toml` — so the
capability list is available at the point the probe project is generated. Two
plausible spellings:

* have the probe read `[system].features` and emit the same
  `NanoRosCapabilities.cmake` call a real build makes, so ONE lowering serves
  both (the phase-314 argument);
* or pass the lowered defines through the hook that now exists per crate
  (`NROS_EXTRA_C_FEATURES` / `NROS_EXTRA_CPP_FEATURES`, issue 0542).

The first is preferable: the second re-derives the lowering and is how the
three-way disagreement phase-314 deleted got started.

## Acceptance

* `nros sync examples/workspaces/safety` regenerates all four C/C++ sidecars
  with no "no producer" line.
* A component using a capability-gated API probes on a workspace that declares
  the capability, and still fails to probe on one that does not — the second
  half matters, since a probe that silently enables everything would hide a real
  configuration error.
* No second copy of the `feature → define` lowering.
