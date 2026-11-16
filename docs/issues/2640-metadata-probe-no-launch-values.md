---
id: 2640
title: "The C/C++ metadata probe cannot construct a node that declares parameters without defaults, or more of them than a default store holds"
status: open
type: bug
area: [cli, params, cmake]
severity: medium
found: 2026-11-16
related: [phase-478]
---

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md).**
Reported by the Autoware Safety Island (phase-15 P3) on nano-ros 5b9e8844a4.

## What happens

The metadata probe (`orchestration/metadata_probe_cmake.rs`, run from
`metadata_refresh.rs`) builds a one-node host plan for each C/C++ component
(`probe_plan`) with `params: Vec::new()` and `param_services: false`, and
links it against a store at the default knobs. An rclcpp-shaped Autoware
node therefore runs its constructor with no launch values and a store
smaller than its declarations:

- a `declare_parameter<T>(name)` with NO default reads `T()`: the island's
  controller divides by `ctrl_period = 0` and sizes a deque from the NaN;
- past the store's capacity every further declaration silently returned its
  compiled default (GitHub #4018; since issue 2634 it stops the run, so the
  component goes unprobed instead).

So the node cannot be probed, and the island duplicates every declaration's
default in C++ to keep the probe alive.

## Store size (resolved 2026-11-16)

The probe's build now states `NROS_MAX_PARAMETERS=1024`
(`params.max_parameters`) beside `NROS_EXECUTOR_MAX_CBS=64`
(`pin_probe_env`), for the reason issue 1969 pinned the executor's: the probe
is what produces the declaration count, so nothing can derive the size for it.
Both knobs are part of `probe_inputs_key`, so a sidecar from a probe built
under other sizes reads stale and is probed again. The probe's cargo directory
was already keyed on them (`nros_knob_key_fields`).

## Launch values (open)

A design is in review as its own draft change: the probe seeds the component's launch values from the launch
RECORD, a component whose instances disagree is refused with a plain message,
and the sidecar records `probe.seeds`.
