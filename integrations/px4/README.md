# PX4 integration (Phase 139.5)

This directory hosts the generic PX4 external-module template that
consumes nano-ros. It is a COPY-OUT TEMPLATE — meant to be vendored
into a downstream PX4 app tree, not built in place.

## Usage

```bash
# 1. Copy the template into your PX4 app's tree.
cp -r <nano-ros>/integrations/px4/module-template ./px4-modules

# 2. Adjust src/CMakeLists.txt + src/modules/nano_ros_app/ to taste.
#    Rename the module, list your .cpp sources, set MAIN.

# 3. Point PX4's external-module discovery at it.
export EXTERNAL_MODULES_LOCATION=$PWD/px4-modules
export NANO_ROS_DIR=/path/to/nano-ros

# 4. Build PX4 SITL with the module included.
make -C $PX4_AUTOPILOT_DIR px4_sitl_default
```

## Layout (PX4 contract)

PX4 enforces an exact directory shape for external modules:

```
$EXTERNAL_MODULES_LOCATION/
└── src/
    ├── CMakeLists.txt                       (lists module names)
    └── modules/
        └── <name>/
            ├── CMakeLists.txt               (px4_add_module(...))
            └── <name>.cpp                   (module body)
```

`px4_add_module(MAIN <name>)` must match the directory name and the
C++ entry-point symbol `<name>_main`. The template ships with
`nano_ros_app`; rename consistently in all three places when
adapting.

## Reference

The validated end-to-end shape (with the Rust-side
`nros-rmw-cffi_register` resolved from `libnros_cpp.a`) lives at
`examples/px4/cpp/uorb/` (Phase 115.K.4.5 + 131.C.2). That tree is
the working SITL validation; this template is the generic shell.

## Registry note

PX4 has no central module registry. Downstreams vendor the template
via git submodule or `cp -r`. See `docs/release/registry-publishing.md`
for the comparison across ecosystems.
