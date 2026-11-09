---
id: 2344
title: "Fixture builds stop on issue 2278's bundled-interfaces refusal on a host with no ROS discoverable, so tier 2 reaches no cell"
status: open
type: bug
severity: high
area: [ci, testing, cmake, runner]
related: [2278, 2302, 2356, phase-528, phase-489, phase-466]
found: 2026-11-07
---
**Home: [phase-489](../roadmap/phase-489-ci-build-speedup.md)** (the runner image is CI build infrastructure).

## Reopened 2026-11-07: a runner whose image predates the fix

Tier-2 run 42393593998 (workflow_dispatch on main 15ae9b72b0, 16:53 UTC), job
128466122834, ran on **`nano-ros-runner-newslab-server-241`**, not on 118. It
stopped in `just build tier2` → `build-compile-check-fixtures` (line 1985, exit
2), on this issue's refusal:

```text
no-ros: the message package `diagnostic_msgs` is on neither
NROS_INTERFACE_SEARCH_PATH nor AMENT_PREFIX_PATH, and nano-ros does not
generate it from its bundled copy in place of your ROS install (issue 2278).
```

So the fix holds only for a runner whose image was rebuilt after it. The
resolution was measured on 118's image, rebuilt at 07:53Z. The 241 runner's
image is older, and the `source ./activate.sh` path the resolution relied on
did not set `AMENT_PREFIX_PATH` in that job. It is NOT issue 2356: the run
never reached `check::default`.

**What would close it:** the 241 runner image rebuilt from main (which now
carries `ENV AMENT_PREFIX_PATH` in `scripts/ci/runner-container.sh`), the same
for 118 (its handover lists that rebuild as pending), and one tier-2 run on
each that gets past `build-compile-check-fixtures`.


## Measured

`run-matrix` run 42341007273 (dispatch, 2026-11-07 05:02 UTC, `main`
`ebeb22e0ef`) stopped in `just build tier2`, in `build-compile-check-fixtures`.
Both `cmake-configure` rows, `cpp_port_rclcpp_compat_smoke` and
`cpp_port_topic_state_monitor`, failed with:

```
CMake Error at cmake/NanoRosBundledInterfaces.cmake:109 (message):
  nano-ros: the message package `diagnostic_msgs` is on neither
  NROS_INTERFACE_SEARCH_PATH nor AMENT_PREFIX_PATH, and nano-ros does not
  generate it from its bundled copy in place of your ROS install (issue 2278).
```

That refusal landed on main at 02:02 UTC (`16bc1646f9`, issue 2278). The
self-hosted runner's image installed the zenoh router and only four interface
packages (std_msgs, geometry_msgs, sensor_msgs, example_interfaces, for
`check-cli-tests`), and `scripts/ci/runner-container.sh` stated that
AMENT_PREFIX_PATH was deliberately not set. So the first fixture row that
generated a package outside those four stopped the lane.

Reproduced on a host with no ROS (newslab-server-241, AMENT_PREFIX_PATH unset):
`NROS_FIXTURE_IDS=cpp_port_rclcpp_compat_smoke,cpp_port_topic_state_monitor
scripts/build/compile-check-fixtures.sh` fails both rows the same way, on
`builtin_interfaces` there.

## Ruling (user, 2026-11-07)

"The msgs must respect those from ROS. We use bundled msgs only to work around
the internal dep issues." So the fix belongs in the runner image, not in the
fixtures: they do not opt into the bundled interfaces. (That alternative, the
fixture builders falling back to the bundled copy when no ROS is discoverable,
was built on branch `fix-2344-fixture-interfaces` and rejected.)

## Resolution

Two halves, both in the runner image:

1. **Every interface package a build names is installed.**
   `[prereq.ros-interface-packages]` in `nros-sdk-index.toml` lists every
   package the CLI bundles (`packages/cli/interfaces/*`), `diagnostic_msgs`
   included, and the image installs that list. The runner image on
   newslab-118 was rebuilt with it at 07:53 UTC.
2. **AMENT_PREFIX_PATH names the ROS prefix.** A tier-2 job already runs
   `source ./activate.sh`, which sources `/opt/ros/$ROS_DISTRO/setup.bash`, so
   the job's builds saw the prefix once the packages were in it. The image now
   also sets `ENV AMENT_PREFIX_PATH=/opt/ros/<distro>`, so a shell that does not
   source activate.sh (a `docker exec`, a step that skips it) agrees, and the
   generator's comments no longer say the opposite of the ruling.

Verified: tier-2 run 42364894798 on `nano-ros-runner-newslab-118-server`
(dispatch, `9d0bd000a8`, after the 07:53 rebuild) printed `All test fixtures
built.` Both `cpp_port_*` rows configured and built, generating
`diagnostic_msgs` from the ROS install
(`Compiling nano-ros-cpp-ffi-diagnostic_msgs`), and the lane went on to
`check::default`, where it stopped on a different cause, issue 2356 (the
runner's /tmp tmpfs filled).
