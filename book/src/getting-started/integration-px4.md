# PX4 (integration shell)

> **Contributor docs?** Building nano-ros's PX4 example from this
> repository is covered at [PX4 (contributor)](./px4.md).

nano-ros lifts the PX4 `EXTERNAL_MODULES_LOCATION` pattern into a
generic copy-out template, shipped in the SDK root at
`integrations/px4/module-template/`. Downstream PX4 apps copy the
template, customise it, and point `EXTERNAL_MODULES_LOCATION` at
their copy.

## Prereqs

- PX4-Autopilot checkout (`git clone https://github.com/PX4/PX4-Autopilot`)
- The full PX4 SITL build toolchain (`make px4_sitl_default` works
  standalone)
- The `nros` CLI on PATH ([Installation](./installation.md)); the
  template finds the SDK by asking `nros sdk-root`

## One-liner vendor

```bash
cp -r "$(nros sdk-root)/integrations/px4/module-template" ./my-px4-modules
```

(The template ships in releases cut after 0.6.0-nros4; with an older
release, copy it from
[GitHub](https://github.com/NEWSLabNTU/nano-ros/tree/main/integrations/px4/module-template).)

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
make -C $PX4_AUTOPILOT_DIR px4_sitl_default
```

> **Not replayed.** No probe track runs the PX4 SITL build yet, so this line is unmeasured on the installed track ([issue 2176](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/archived/2176-unprobed-fvp-sitl-flash-lines.md)).

PX4's build picks up the module via `<location>/src/CMakeLists.txt`.
The module CMakeLists resolves the nano-ros SDK root by running
`nros sdk-root` (or takes `-Dnano_ros_ROOT=<dir>`), and pulls in the
nano-ros C++ headers from there.

## Reference

The validated end-to-end SITL pattern (with the Rust-side
`nros-rmw-cffi_register` weak-stub trick) lives at
[`packages/testing/nros-px4-register-check/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/testing/nros-px4-register-check)
on GitHub. Use that as the source of truth for the
register-check shape; this template is the generic shell for
downstreams writing their own modules.

## Registry note

PX4 has no central module registry. Downstreams copy the template
(`cp -r`) into their own tree. See
[Registry Publishing](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/release/registry-publishing.md) for
the cross-ecosystem comparison.
