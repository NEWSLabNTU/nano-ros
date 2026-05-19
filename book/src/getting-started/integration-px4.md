# PX4 (integration shell)

> **Contributor docs?** Building nano-ros's PX4 example from this
> repository is covered at [PX4 (contributor)](./px4.md).

nano-ros lifts the PX4 `EXTERNAL_MODULES_LOCATION` pattern into a
generic copy-out template under
`integrations/px4/module-template/`. Downstream PX4 apps vendor the
template, customise it, and point `EXTERNAL_MODULES_LOCATION` at
their copy.

## Prereqs

- PX4-Autopilot checkout (`git clone https://github.com/PX4/PX4-Autopilot`)
- The full PX4 SITL build toolchain (`make px4_sitl_default` works
  standalone)

## One-liner vendor

```bash
cp -r /path/to/nano-ros/integrations/px4/module-template \
      ./my-px4-modules
```

## Layout (PX4 enforces this exactly)

```
my-px4-modules/
└── src/
    ├── CMakeLists.txt                       (lists module names)
    └── modules/
        └── nano_ros_app/
            ├── CMakeLists.txt               (px4_add_module)
            └── nano_ros_app.cpp             (module body)
```

Rename `nano_ros_app` consistently in all three places when adapting
for your own module.

## Build

```bash
export EXTERNAL_MODULES_LOCATION=$PWD/my-px4-modules
export NANO_ROS_DIR=/path/to/nano-ros
make -C $PX4_AUTOPILOT_DIR px4_sitl_default
```

PX4's build picks up the module via `<location>/src/CMakeLists.txt`.
The module CMakeLists pulls in nano-ros C++ headers via
`$NANO_ROS_DIR`.

## Reference

The validated end-to-end SITL pattern (with the Rust-side
`nros-rmw-cffi_register` weak-stub trick) lives at
`examples/px4/cpp/uorb/`. Use that as the source of truth for the
register-check shape; this template is the generic shell for
downstreams writing their own modules.

## Registry note

PX4 has no central module registry. Downstreams vendor the template
via `git submodule` or `cp -r`. See
[Registry Publishing](../../docs/release/registry-publishing.md) for
the cross-ecosystem comparison.
