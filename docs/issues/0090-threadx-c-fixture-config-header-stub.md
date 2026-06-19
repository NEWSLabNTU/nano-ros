---
id: 90
title: ThreadX-linux C fixture compiles against the nros_config_generated.h stub (0088 residual on the threadx header path)
status: open
type: bug
area: cmake
related: [0088, phase-258]
---

## Symptom (2026-06-19)

`just build-test-fixtures` (the `threadx_linux` leaf) — the C talker fixture fails:

```
In file included from .../nros-c/include/nros/types.h:24,
                 from .../nros-c/include/nros/component.h:45,
                 from examples/threadx-linux/c/talker/src/Talker.c:14:
.../nros-c/include/nros/nros_generated.h:940:20: error: 'SESSION_OPAQUE_U64S' undeclared here (not in a function)
  ... (every *_OPAQUE_U64S + ActionServerRawHandle undeclared)
```

i.e. `Talker.c` compiled against the in-tree **stub** `nros_config_generated.h`
instead of the per-build header — the same failure class as [[0088-zephyr-c-fixture-nros-config-generated-stub]].

## Relation to 0088

Issue 0088's fix closed the **native / cpp / mixed (Corrosion)** path: the mirror
became a first-class `OUTPUT` + `nros_{c,cpp}_config_header` target and
`NanoRosNodeRegister.cmake` deps every `${_NRC_SOURCES}` consumer on it (deferred).
That helper also lists Zephyr's `nros_{c,cpp}_cargo_build` names. **ThreadX-linux
uses yet another header-provisioning path** (its own board cmake +
`nano_ros_node_register` carrier executable, see the threadx branch in
`cmake/NanoRosNodeRegister.cmake` ~line 445 + `cmake/platform/nano-ros-threadx.cmake`)
whose generator target / generated-header include dir the helper's dep list does
not match — so the carrier TU still races the header and reads the stub.

## Fix direction

Identify the threadx nros-c/nros-cpp header generator target + the include dir it
writes to, and ensure the threadx carrier (and any threadx component lib) gets a
real producer→consumer edge to it — either by adding the threadx generator target
name to `_nros_node_register_apply_config_header_deps`'s guarded list, or (more
robust) an `OBJECT_DEPENDS` file-level edge on the generated header. Mirror the
0088 native fix's structure. Confirm on the threadx-linux fixture + the
qemu-riscv64-threadx path.

## Scope

Surfaced running host `test-all` for phase-258 on a host without the Zephyr SDK.
Orthogonal to phase-258. Part of the broader "per-platform header-wiring is
inconsistent" theme (0088) — native is fixed, threadx + zephyr remain.
