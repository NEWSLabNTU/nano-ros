---
id: 1681
title: "A Zephyr C++ image whose bringup declares `param_services` fails to link: `nros_cpp_register_parameter_services` undefined"
status: open
type: bug
area: [build, zephyr]
severity: medium
found: 2026-10-05
related: [1649, 1529, phase-461]
---

## What

Measured 2026-10-04 on a TEMPORARY image of `examples/workspaces/cpp`
(`zephyr_svc`: native_sim, zenoh, the service server's node, `[system]
features = ["param_services"]`), built through `nros build zephyr_svc -- -d
<dir>` in a worktree-local Zephyr workspace:

```
.../build/m1649-west/zephyr_svc_entry_nros_main_generated.cpp:37: undefined reference to `nros_cpp_register_parameter_services'
```

The generated entry calls `nros_cpp_register_parameter_services(executor)`
because the bringup declares `param_services` (that is the codegen rule pinned
by `codegen/entry/emit/tests_c.rs`). The symbol is defined in
`packages/api/nros-cpp/src/params_shim.rs` under nros-cpp's `param-services`
feature. `zephyr/CMakeLists.txt` appends `,alloc,param-services` to the nros-cpp
feature list when `"param_services" IN_LIST NANO_ROS_FEATURES`, so either that
list does not hold the bringup's features on this road, or the archive that is
linked is not the one built with them. Not diagnosed further: it was found
while measuring issue 1649, whose build-script outputs are produced before the
link and were diffed regardless.

## Reproduce

Add an image to `examples/workspaces/cpp/src/demo_bringup/system.toml` with
`board = "native_sim"`, `rmw = "zenoh"`, a launch file running the service
server, and `features = ["param_services"]`; `nros sync`; `nros build <image>`.

## Acceptance

That image links; a fixture row or a `check-compile-smoke`-tier build holds it,
because no in-tree Zephyr C++ image declares `param_services` today, which is
why nothing caught this.
