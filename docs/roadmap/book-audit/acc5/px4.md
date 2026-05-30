BLOCKERS (fixed in `53ef20a53`)
1. Tree diagram (L22–32) showed `px4-modules/nano-ros/src/…` but the three cmake invocations (L53, L59, L78) passed `-DEXTERNAL_MODULES_LOCATION=$PWD/px4-modules` — the parent dir, missing the `nano-ros/` level. PX4's root `CMakeLists.txt:437` does `add_subdirectory("${EXTERNAL_MODULES_LOCATION}/src" …)`, so the parent-dir form fails configure verbatim:
   ```
   CMake Error at CMakeLists.txt:437 (add_subdirectory):
     add_subdirectory given source ".../px4-modules/src"
     which is not an existing directory.
   ```
   The template's own header (`integrations/px4/module-template/CMakeLists.txt:14–15`) says the template IS the `px4-modules/nano-ros` dir. Fixed: all 3 cmake snippets now pass `$PWD/px4-modules/nano-ros`; added an inline note explaining the resolution.

FRICTION (fixed in `53ef20a53`)
- L128 said "check the PX4 boot log for `nano-ros: register failed`" — the template actually logs `nros_rmw_uorb_register() -> <rc>` on startup; greppers for the doc's string found nothing. Updated the troubleshoot bullet to match the template's real log line.

CLARITY
- CLEAR after the fix. The tree, the cmake var, and the PX4 add_subdirectory contract are now consistent.

MISSING STEPS
- None.

WORKS
- `just setup px4` (provisions submodules + Python deps, ~4 min).
- `just px4 doctor` (all green; arm-none-eabi-gcc available).
- Readiness signal string `"nano-ros uORB backend registered"` verified at `integrations/px4/module-template/src/modules/nano_ros_app/nano_ros_app.cpp:34`.
- All GitHub source links resolve.
- `NANO_ROS_DIR` env/cache precedence claim matches template source (post-D.11).
- Coverage-matrix carve-out (C++-only) consistent with `examples/README.md`.

ENVIRONMENTAL (not doc bugs)
- PX4-Autopilot ships as a submodule; full toolchain build was not attempted (~10 min cross-compile, beyond a re-audit budget).
- A secondary `src/lib/version/CMakeLists.txt` error fires when PX4 is copied out of its submodule layout; artefact of the audit scratch setup, not a doc bug.

Acceptance bar (0 BLOCKERS): MET on re-run after `53ef20a53`.

LAST COMMAND: cmake -B build -S PX4-Autopilot -DCONFIG=px4_fmu-v5_default -DEXTERNAL_MODULES_LOCATION=$PWD/px4-modules/nano-ros -DNANO_ROS_DIR=… -DNANO_ROS_RMW=zenoh
LAST EXIT CODE: 0
