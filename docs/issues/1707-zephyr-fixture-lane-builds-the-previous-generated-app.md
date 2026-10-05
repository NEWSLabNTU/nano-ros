---
id: 1707
title: "The Zephyr fixture lane builds an already-configured image from the PREVIOUS generated west application: a declaration change arrives one lane run late"
status: open
type: bug
area: [zephyr, build, testing]
severity: medium
found: 2026-10-06
related: [1702, 1681, 1288, 0196]
---

## What was measured

`nros build` generates each Zephyr image's west application under
`<ws>/build/zephyr-zenoh/<image>_entry/` (phase-470 W5). That includes
`NANO_ROS_FEATURES` and, since issue 1702, the `CONFIG_NROS_CAPABILITY_*`
Kconfig assignments. `scripts/build/zephyr-fixture-run-one.sh` re-runs an
already-configured leaf with plain `ninja -C <build_dir>` whenever its record
signature is unchanged, and editing `system.toml` does not change that
signature.

In `just zephyr build-fixtures` on `examples/workspaces/features`, every
generated application's `CMakeLists.txt` was rewritten while the run was
already underway, AFTER some leaves' `ninja` had started or finished. So the
images built from the previous file. Two independent sightings:

1. **Capability Kconfig (issue 1702 measurements).** After
   `features = ["param_services"]` became `[]`, one lane run left:

   ```
   cpp-params: app[05:56:15] PARAM_SERVICES n | build.ninja[05:56:18] .config: CONFIG_NROS_CAPABILITY_PARAM_SERVICES=y
   rs-params:  app[05:56:15] PARAM_SERVICES n | build.ninja[05:51:41] .config: ...=y
   ```

   The cpp leaf's ninja-triggered reconfigure wrote `extra_kconfig_options.conf`
   with the OLD `y`, because the cache variable the new file sets was not
   defined during that configure. The Rust leaves never reconfigured, because
   their ninja ran before the rewrite. A plain `cmake <build_dir>` afterwards
   applied the new value at once.
2. **`NANO_ROS_FEATURES`.** After `[image.zephyr_cpp_params]`'s bringup dropped
   `param_services` (keeping `lifecycle`), the image built nros-cpp with
   `param-services,...` and no `lifecycle-services`, which is the previous
   run's axes, and failed to link
   (`undefined reference to nros_cpp_lifecycle_autostart`). The next run, with
   the axes restored, failed the other way
   (`undefined reference to nros_cpp_register_parameter_services`). A
   `cmake <build_dir>` re-configure gave the right feature string both times.

So a link failure or a wrong heap here can be a museum configure rather than a
code defect. A fresh build dir is unaffected (it takes the `nros build` path,
which writes the application BEFORE configuring), which is why CI does not see
it.

## Not yet known

Which step rewrites the applications mid-run. It is not the leaf being built.
The candidates are a parallel leaf's `nros build` (a sibling image whose
signature did change regenerates the workspace's applications) or a prep step.
The timestamps show the write lands inside the make scheduler's window.

## Fix direction

The leaf's freshness decision must cover what its configure reads. Either
generate every image's application BEFORE the scheduler starts and include the
generated file in the leaf signature, or have the ninja path compare the
application's mtime against `build.ninja` and fall back to `nros build`. Gate
it with a fixture that toggles an axis and builds once.
