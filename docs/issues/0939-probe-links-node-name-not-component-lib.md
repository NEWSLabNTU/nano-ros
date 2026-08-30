---
id: 939
title: "The metadata probe links the node NAME, which is not a target — so
  every C/C++ component reports `no producer`, once, and then silently"
status: open
type: bug
area: tooling
related: [issue-0409, rfc-0057]
---

## Symptom

`nros sync` reports no source metadata for a C/C++ component:

```
sync: source metadata — no producer for controller_pkg::controller
      (metadata probe build failed for `controller_pkg::controller` (exit 2):
      ...
      /usr/bin/ld: cannot find -lcontroller: No such file or directory
```

The workspace still builds and the image still runs, so nothing draws attention
to it. What is lost is the sidecar the probe exists to produce — the component's
own declared metadata — and `sync` carries on with a model that is missing it.

## Cause

`NanoRosNodeRegister.cmake` names the component library
`${PROJECT_NAME}_${NAME}_component`:

```cmake
set(_lib "${PROJECT_NAME}_${_NRC_NAME}_component")
```

and guarantees a target by that name in BOTH modes — under `EXISTING_TARGET` it
adds an INTERFACE library of that name linking the caller's target, so the
spelling is universal.

The workspace scanner did not use it. Both paths defaulted to the bare node
name:

```rust
// nano_ros_node_register
library_target: args.single("TARGET").or_else(|| Some(name.clone())),
// nano_ros_add_node
library_target: Some(name.clone()),
```

The bare name is not a target, so the generated probe emits
`target_link_libraries(probe_… PRIVATE controller)` and the link fails at
`-lcontroller`. Note the `TARGET` fallback never applied either: the CMake verb
parses `EXISTING_TARGET`, not `TARGET`, and nothing in-tree passes `TARGET` at
all — so the wrong default is what every component got.

## Why it stayed invisible

The failure is cached. After the first attempt the probe writes

```
<pkg>/metadata/<node>.json.unprobeable
```

keyed by a source hash, and every later sync prints

```
sync: source metadata — no producer for … (probe failed at this source last
      sync; unchanged)
```

which reads like a note about an unchanged source rather than a build that is
still broken. The underlying linker error is printed once, in the run that
first hit it, and never again — so on any machine that has synced before, the
evidence is gone. It took a submodule pin bump (which changed the hash and
re-ran the probe) to surface it.

## Fix

Derive the target the way the CMake does, in one place, honouring an explicit
`TARGET` where a caller gives one:

```rust
fn component_library_target(package: &str, name: &str) -> String {
    format!("{package}_{name}_component")
}
```

Two regression tests pin it, one per verb.

## Not fixed here

With the link corrected the probe gets further and then fails on the component
itself, in at least one consumer: a package whose `CMakeLists.txt` relies on
variables and targets its PARENT sets (`EIGEN3_INCLUDE_DIR`, an
`autoware_msgs` INTERFACE target) cannot be built as a standalone
subdirectory, which is exactly how the probe builds it. That is a consumer-side
property, not a scanner bug, but it means this fix alone does not make every
component probeable — it makes the probe attempt the right thing and fail
honestly, instead of failing on a name that could never have resolved.
