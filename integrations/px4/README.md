# PX4 integration (Phase 139.5 + 212.H.7)

PX4 is a **HOOKLESS vendor** in the Phase 212 RTOS-integration taxonomy
(see `docs/design/0003-rtos-integration-pattern.md` §1 + §3): its CMake +
Kconfig configure step doesn't have a hook rich enough to read
`system.toml` from inside the vendor tool. Instead the codegen runs
**ahead of vendor**: `nros codegen-system --ahead-of-vendor --target px4`
emits one PX4 module dir per `[[component]]` into
`$PX4_DIR/src/modules/nros_<name>/`, then the user runs
`make px4_sitl_default` from the PX4 tree.

Same precedent as the PlatformIO adapter at `integrations/platformio/`.
C++-only per Phase 115.K.4 (the PX4 surface collapsed to a single
C++ uORB port; there is no Rust uORB on the SITL path).

## User incantation (Phase 212.H.7 + M-F.8)

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

# 3. Render the SITL board overlay enabling those modules (Phase 212.M-F.8).
#    PX4's SITL build won't pick the new module dirs up unless their
#    Kconfig switches are flipped ON in a `.px4board` fragment under
#    `boards/px4/sitl/`. The helper walks `<px4>/src/modules/nros_*/`
#    and emits one `CONFIG_MODULES_NROS_<UPPER>=y` line per dir; append
#    the rendered fragment onto the SITL board file of your choice
#    (PX4 reads `.px4board` files as a single Kconfig defconfig, so
#    concatenation is the supported merge):
integrations/px4/sitl-overlay/render-overlay.sh \
    --px4-dir $PX4_AUTOPILOT_DIR \
    >> $PX4_AUTOPILOT_DIR/boards/px4/sitl/default.px4board

# 4. Build SITL — PX4's own walker now picks the new modules up.
make -C $PX4_AUTOPILOT_DIR px4_sitl_default
```

`nros codegen-system --ahead-of-vendor --target px4` operates entirely
**outside the nano-ros tree** — it writes into `$PX4_AUTOPILOT_DIR`
directly, mirroring the PlatformIO `extra_script` path. The
`render-overlay.sh` helper at `integrations/px4/sitl-overlay/`
similarly never touches the vendored PX4 tree itself — it only reads
from `<px4>/src/modules/` and emits a fragment the operator then
appends to a `.px4board` of their choice.

### TODO — future CLI-side automation

Step 3 (overlay render) lives outside `nros` today because the
codegen verb for the PX4 target originally landed in the standalone
`nros-cli` repo (archived; the CLI merged in-tree under
`packages/cli/` in Phase 218) as the single-purpose module-dir emit.
A `--board-overlay <path>` flag on `nros codegen-system --target px4`
would fold the overlay-render step into the same invocation (one
tool, one walk). Tracked in Phase 212.M-F.8.

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
│   ├── src/                             ← legacy 139.5 layout
│   │   ├── CMakeLists.txt
│   │   └── modules/nano_ros_app/...
└── sitl-overlay/                        ← 212.M-F.8 SITL board overlay
    ├── nros.px4board.in                 ← Kconfig defconfig template
    └── render-overlay.sh                ← walks <px4>/src/modules/nros_*/
                                          and renders the fragment
```

## LoC budget

`tokei integrations/px4/` reports the entire shim ≤300 LoC per the
Phase 212 §H.8 budget (the component skeleton ≤80 LoC; the 139.5
legacy template another ~110; the M-F.8 SITL overlay another ~100).

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
template via git submodule / `cp -r`. In either path the SITL
build still needs the `sitl-overlay/render-overlay.sh` step to
flip the per-module Kconfig switches; see the M-F.8 user-incantation
block above. `docs/release/registry-publishing.md` carries the
cross-ecosystem comparison.
