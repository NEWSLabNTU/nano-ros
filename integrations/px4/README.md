# PX4 integration (Phase 139.5 + 212.H.7)

PX4 is a **HOOKLESS vendor** in the Phase 212 RTOS-integration taxonomy
(see `docs/design/rtos-integration-pattern.md` §1 + §3): its CMake +
Kconfig configure step doesn't have a hook rich enough to read
`system.toml` from inside the vendor tool. Instead the codegen runs
**ahead of vendor**: `nros codegen-system --ahead-of-vendor --target px4`
emits one PX4 module dir per `[[component]]` into
`$PX4_DIR/src/modules/nros_<name>/`, then the user runs
`make px4_sitl_default` from the PX4 tree.

Same precedent as the PlatformIO adapter at `integrations/platformio/`.
C++-only per Phase 115.K.4 (the PX4 surface collapsed to a single
C++ uORB port; there is no Rust uORB on the SITL path).

## User incantation (Phase 212.H.7)

```bash
# 1. Bake nano-ros component module dirs into the PX4 source tree.
nros codegen-system --ahead-of-vendor \
    --target px4 \
    --workspace .                       \
    --bringup demo_bringup              \
    --out $PX4_AUTOPILOT_DIR/src/modules

# 2. Each `[[component]].name` from `demo_bringup/system.toml` shows up as
#    $PX4_AUTOPILOT_DIR/src/modules/nros_<component>/
#                                     ├── CMakeLists.txt   (px4_add_module)
#                                     ├── Kconfig          (per-module switch)
#                                     └── <component>.cpp  (rendered body)

# 3. Build SITL — PX4's own walker picks the new modules up.
make -C $PX4_AUTOPILOT_DIR px4_sitl_default
```

`nros codegen-system --ahead-of-vendor --target px4` operates entirely
**outside the nano-ros tree** — it writes into `$PX4_AUTOPILOT_DIR`
directly, mirroring the PlatformIO `extra_script` path.

## Legacy template (Phase 139.5) — `EXTERNAL_MODULES_LOCATION`

For the pre-212 single-module copy-out shape, this dir still ships the
original Phase 139.5 fixed-`nano_ros_app` template at
`module-template/src/modules/nano_ros_app/`. PX4's
`EXTERNAL_MODULES_LOCATION` discovery walks `module-template/src/` and
pulls in modules listed under `config_module_list_external`:

```bash
cp -r <nano-ros>/integrations/px4/module-template ./px4-modules
export EXTERNAL_MODULES_LOCATION=$PWD/px4-modules
export NANO_ROS_DIR=/path/to/nano-ros
make -C $PX4_AUTOPILOT_DIR px4_sitl_default
```

The 212.H.7 hookless path supersedes this — the legacy template remains
in tree for back-compat with `tests/integration_px4.rs` (the Phase 139.6
smoke test).

## Files

```
integrations/px4/
├── README.md                            ← this file
├── module-template/
│   ├── CMakeLists.txt                   ← top-level dispatch (139.5)
│   ├── Kconfig                          ← 212.H.7 top-level Kconfig stub
│   ├── component-skeleton/              ← 212.H.7 per-component skeleton
│   │   ├── CMakeLists.txt               ← px4_add_module(MAIN @COMPONENT_NAME@)
│   │   ├── Kconfig                      ← per-component Kconfig
│   │   └── component.cpp.template       ← module body w/ placeholders
│   └── src/                             ← legacy 139.5 layout
│       ├── CMakeLists.txt
│       └── modules/nano_ros_app/...
```

## LoC budget

`tokei integrations/px4/module-template/` reports the entire shim
≤200 LoC per the Phase 212 §H.8 budget (the component skeleton is
≤80 LoC; the 139.5 legacy template another ~110).

## Reference

The validated SITL build of the legacy 139.5 path lives at
`examples/px4/cpp/uorb/` (Phase 115.K.4.5 + 131.C.2). That tree is
the SITL-validated reference; this template is the generic shell.

`packages/testing/nros-tests/tests/phase212_h7_px4.rs` (212.H.7) and
`packages/testing/nros-tests/tests/integration_px4.rs` (139.6) cover
the post-codegen module-dir shape + the legacy
`EXTERNAL_MODULES_LOCATION` path respectively.

## Registry note

PX4 has no central module registry. Downstreams either let
`nros codegen-system --ahead-of-vendor` emit dirs into
`$PX4_AUTOPILOT_DIR/src/modules/` per-build, or vendor the legacy
template via git submodule / `cp -r`. See
`docs/release/registry-publishing.md` for the cross-ecosystem comparison.
