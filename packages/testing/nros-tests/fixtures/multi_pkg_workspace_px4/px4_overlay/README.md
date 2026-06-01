# `px4_overlay/` — PX4 module-emit landing zone

`nros codegen-system --ahead-of-vendor --target px4` writes the rendered
per-component module dirs (one per `[[component]]` in
`../demo_bringup/system.toml`) under `$PX4_AUTOPILOT_DIR/src/modules/`
DIRECTLY. This directory is a documentation placeholder showing the
expected post-codegen shape; nothing is committed under it.

Expected layout after a real run against this fixture:

```
$PX4_AUTOPILOT_DIR/src/modules/
├── nros_talker/
│   ├── CMakeLists.txt        ← rendered from integrations/px4/module-template/component-skeleton/CMakeLists.txt
│   ├── Kconfig
│   └── talker.cpp            ← rendered from component.cpp.template
└── nros_brake_arbiter/
    ├── CMakeLists.txt
    ├── Kconfig
    └── brake_arbiter.cpp
```

Then `make -C $PX4_AUTOPILOT_DIR px4_sitl_default` picks the new
modules up via PX4's normal `src/modules/` walker. See
`docs/design/rtos-integration-pattern.md` §3 row "PX4" for the
hookless-vendor pattern.
