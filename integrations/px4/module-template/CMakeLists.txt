# Phase 139.5 — PX4 module template (top-level dispatch).
#
# Generic copy-out template for a PX4 external module that consumes
# nano-ros. Users point PX4's `EXTERNAL_MODULES_LOCATION` env var at
# THIS directory; PX4 then walks `src/CMakeLists.txt` and pulls in
# every entry listed under `config_module_list_external`.
#
# Pattern lifted from examples/px4/cpp/uorb/ (Phase 131.C.2). The
# example tree there stays as the validated reference; this template
# is the generic copy-out form for new PX4 modules.
#
# Usage (in a downstream tree):
#
#   cp -r <nano-ros>/integrations/px4/module-template <your-app>/px4-modules
#   export EXTERNAL_MODULES_LOCATION=$PWD/px4-modules
#   make -C $PX4_AUTOPILOT_DIR px4_sitl_default

# PX4 expects external modules under `<location>/src/modules/<name>/`.
# `src/CMakeLists.txt` enumerates the modules; `src/modules/<name>/`
# carries the module body. Nothing happens at the top level.
