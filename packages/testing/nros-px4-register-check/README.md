# nros-px4-register-check

A PX4 module whose **build is the assertion**: it compiles the whole
`nros-rmw-uorb` C++ backend inline inside a real `px4_add_module()` context and
calls `nros_rmw_uorb_register()`, so the linker has to resolve every entry point
the cffi adapter dispatches to.

Not a library and not an example — nothing imports it, and it produces no
artifact anyone runs on its own. It is a link/registration gate, which is why
phase-316 W3.1 moved it out of `examples/px4/cpp/uorb/` and under
`packages/testing/` per CLAUDE.md.

## What it covers that nothing else does

This is the only build path that sees **real PX4 headers**:

- `<uORB/uORB.h>` for the data-plane sources (rather than the mock ABI);
- `<uORB/SubscriptionCallback.hpp>` + `WorkQueueManager.hpp` for the push-wake
  glue in `px4_callback_glue.cpp`;
- `<px4_boardconfig.h>`, the per-board Kconfig output that exists only *during*
  a SITL build.

The standalone smoke at `packages/rmw/uorb/nros-rmw-uorb/tests/register_smoke.cpp`
covers the pure data-plane against a mock uORB ABI, and cannot reach any of the
above.

## Running it

```sh
just px4 setup          # once — clones/pins PX4-Autopilot
just px4 build-sitl-cpp # builds SITL with this dir as EXTERNAL_MODULES_LOCATION
```

Cold build ~10 min, warm <2 s. A failure is a real regression in the uORB
backend's PX4-facing surface; there is no "module didn't run" outcome, because
compiling and linking it *is* the test.

## Layout

PX4 requires `<EXTERNAL_MODULES_LOCATION>/src/modules/<name>/CMakeLists.txt`,
so that is exactly the shape here:

```
src/CMakeLists.txt                              config_module_list_external
src/modules/nros_register_check/CMakeLists.txt  px4_add_module(...)
src/modules/nros_register_check/*.cpp           task_main → nros_rmw_uorb_register()
```

Under `examples/` this could not be satisfied directly — the example tree
demanded `<plat>/<lang>/<rmw>/<example>/`, so the real CMakeLists was hoisted up
and a shim at PX4's required path `include()`d it back. Two layout rules, one
directory, one of them served by an indirection. Outside `examples/` only PX4's
applies, and the shim is gone.
