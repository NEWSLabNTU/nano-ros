# examples/px4/cpp/firmware

**In-firmware** PX4 modules: nano-ros running *inside* the autopilot, on the uORB
backend. The sibling `examples/px4/rust/companion/` runs *beside* PX4 over
XRCE-DDS — that is what the `firmware/` vs `companion/` level names (a deployment
axis, not an RMW; see RFC-0026).

## What this demonstrates

uORB is the one backend where nano-ros does not speak a wire protocol:

| | every other backend | uORB |
| --- | --- | --- |
| wire bytes | CDR encoding of the message | the PX4 C struct, verbatim |
| type identity | ROS type name + type hash | `ORB_ID(<topic>)` |
| serialization | encode + decode per sample | none — the payload IS the struct |
| who can read it | another nano-ros / ROS 2 endpoint | **any stock PX4 module** |

So the message type comes from `<uORB/topics/*.h>`, not from `nros generate-*`:
a generated binding would describe a CDR layout, and CDR is exactly what does not
happen here.

## Running it

```sh
just setup px4                 # once
just px4 build-sitl-example    # SITL with this dir as EXTERNAL_MODULES_LOCATION
```

then from the pxh shell:

```
nros_uorb_demo start
listener debug_key_value
```

`listener` is **PX4's own command** and knows nothing about nano-ros. Observed:

```
TOPIC: debug_key_value
 debug_key_value
    timestamp: 16736000 (0.920000 seconds ago)
    value: 5.00000
    key: "nros"
```

The module also subscribes `vehicle_status`, published by PX4's commander, and
logs `nav_state` / `arming_state` — the same property in the other direction.

## Why the proof has to come from the PX4 side

A demo where a nano-ros subscriber reads a nano-ros publisher proves nothing
here. On this backend the interesting failure is a layout or size disagreement
with PX4's `orb_metadata`, and a nano-ros-to-nano-ros test is satisfied
identically by a correct encoding and a broken one — both ends share the bug.
The stock consumer IS the measurement (issue 0351).

## Layout

PX4 requires `<EXTERNAL_MODULES_LOCATION>/src/modules/<name>/`, so that is the
shape here; this directory is the location root. The module follows PX4
convention throughout — `ModuleBase<T>` + `ScheduledWorkItem`, a `Kconfig`,
`PRINT_MODULE_*` usage strings, tab indent, CamelCase file named after the class.

One nano-ros-specific line in its CMakeLists:

```cmake
include("$ENV{NROS_REPO_DIR}/integrations/px4/NanoRosPx4Module.cmake")
nros_px4_add_module(MODULE modules__nros_uorb_demo MAIN nros_uorb_demo
                    BACKENDS uorb SRCS ${CMAKE_CURRENT_LIST_DIR}/NrosUorbDemo.cpp)
```

The helper resolves the prebuilt archives and generated headers, generates the
backend-registration hook, and contributes the uORB backend's own sources.
