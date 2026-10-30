---
id: 2166
title: "The C/C++ metadata probe's recorder holds 32 entities and every parameter is one, so a component declaring more panics and `nros sync` reports `no producer`"
status: open
type: bug
area: [cli, metadata, params]
severity: low
found: 2026-10-30
related: [1419, 1679, 1469, phase-463]
---

## What was measured

Consumer: autoware-safety-island, branch `nano-ros-rebase`, C++ component
`controller_pkg::controller` (Autoware trajectory follower: MPC lateral + PID
longitudinal + vehicle info). Seen on nano-ros 05b7f7d2648f and main 4fcc75d413.

```
sync: source metadata - no producer for controller_pkg::controller
```

The probe panics in `census_hooks.rs` with
`nros metadata mode: recorder rejected parameter ...`. The build and the
image are unaffected, but the image has no source metadata, so nothing that
reads the sidecar can count the component's entities.

The component declares 149 distinct parameters at construction (counted on
the freertos-posix image from the names passed to every
`nros_cpp_node_declare_param_*` call). There are more calls than names,
because the vehicle-info parameters are declared once per sub-controller and
each repeat returns `ALREADY_EXISTS`. It also has 7 subscriptions,
4 publishers and a timer.

## Reading (4fcc75d413; the lines are unchanged on main de7de3d0bc)

* `packages/api/nros/src/node_metadata.rs:88` -
  `pub const DEFAULT_MAX_METADATA_ENTITIES: usize = 32;`, the default
  `MAX_ENTITIES` of `MetadataRecorder` (node_metadata.rs:675).
* `packages/api/nros/src/metadata_mode.rs:70,83` - the recorder that every
  non-Rust adapter feeds is a plain `MetadataRecorder`, so it gets the
  default 32 entities. No knob or per-component size reaches it.
* `metadata_mode.rs:266-298` (`record_parameter`) records each declared
  parameter as an `EntityKind::Parameter` entity in that same 32-row vector,
  one row per CALL (the hook runs before the store answers, so repeats count
  too).
* On overflow `push_entity` returns `NodeMetadataError::Capacity`
  (node_metadata.rs:879-881). Nothing is truncated, and
  `record_parameter` returns `false`.
* `packages/api/nros/src/census_hooks.rs:217-228` (`on_param_declare`) turns
  that `false` into a `panic!` ("a census built from this sidecar would say
  the node declares fewer than it does"). The same applies to topics and
  timers (census_hooks.rs:274-293) and nodes (census_hooks.rs:144-151).
* The probe process dies, so `metadata_refresh` gets no sidecar and
  `ws.rs:3767` prints `no producer for <pkg>::<comp>`.

So the overflow is a deliberate panic, not a silent truncation. But the
capacity is a compile-time default sized for a handful of endpoints, and
since phase-463 W1 parameters share it. Any real C++ component with more
than about 30 declarations (parameters + endpoints + timers) has no metadata.

## Direction

The probe is a host process with `alloc` (`metadata_mode` already leaks
`Box<str>` per type name), so a fixed cap has no reason to be there:

* make the probe recorder's entity and callback storage growable (a `Vec`
  under `alloc`), keeping the fixed-capacity recorder for no_std callers; or
* at least size it from a knob the probe build passes, and say the knob's
  name in the panic message (the current text, "raise the MetadataRecorder
  capacity", names no knob).

The Rust producer's template (`metadata_build.rs:439`,
`nros::MetadataRecorder::default()`) has the same 32 cap, so the same fix
should cover it.

## What would close it

A probe test for a C++ component that declares 200 parameters plus a few
endpoints. Its sidecar lists all of them, and `nros sync` reports it rebuilt,
not `no producer`.
