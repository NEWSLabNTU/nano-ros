---
id: 2344
title: "Fixture builds stop on issue 2278's bundled-interfaces refusal on a host with no ROS discoverable, so tier 2 reaches no cell"
status: resolved
type: bug
severity: high
area: [ci, testing, cmake]
related: [2278, 2302, phase-528, phase-530]
found: 2026-11-07
resolved: 2026-11-07
---

**Home: [phase-530](../../roadmap/phase-530-nros-configure-single-pass.md)**, which owns issue 2278's refusal.

## Measured

`run-matrix` run 42341007273 (dispatch, 2026-11-07 05:02 UTC, `main`
`ebeb22e0ef`) stopped in `just build tier2`, in
`build-compile-check-fixtures`: the `cmake-configure` rows
`cpp_port_rclcpp_compat_smoke` and `cpp_port_topic_state_monitor` both failed

```
CMake Error at cmake/NanoRosBundledInterfaces.cmake:109 (message):
  nano-ros: the message package `diagnostic_msgs` is on neither
  NROS_INTERFACE_SEARCH_PATH nor AMENT_PREFIX_PATH, and nano-ros does not
  generate it from its bundled copy in place of your ROS install (issue 2278).
```

That refusal landed on `main` at 02:02 UTC (`16bc1646f9`, issue 2278). The
commit says "CI images set AMENT_PREFIX_PATH". The self-hosted runner's image
does not, deliberately: `scripts/ci/runner-container.sh` installs the router
and four interface packages and states that AMENT_PREFIX_PATH is NOT set,
because that would make the ROS prefix's message packages discoverable to
every build on the runner. So no fixture build there can name any message
package, and the first row that generates one stops the lane.

Reproduced on a host with no ROS (newslab-server-241, AMENT_PREFIX_PATH unset):
`NROS_FIXTURE_IDS=cpp_port_rclcpp_compat_smoke,cpp_port_topic_state_monitor
scripts/build/compile-check-fixtures.sh` fails both rows the same way (on
`builtin_interfaces` here).

## Resolution

The fixture builders make the choice issue 2278 asks for, once and out loud,
with the refusal's own remedy. `scripts/build/fixture-interfaces.sh`'s
`nros_fixture_interfaces` exports `NROS_INTERFACE_SEARCH_PATH=<repo>/packages/cli/interfaces`
when no ROS is discoverable (AMENT_PREFIX_PATH unset or empty) and the caller
set no search path, and prints that it did. A host with ROS is untouched; its
ament packages still win. A user's build is untouched: the helper is reached
only from the fixture builders.

Called from every fixture builder that configures a build
(`compile-check-fixtures.sh`, `fixtures-build.sh`, `west-fixtures.sh`,
`workspace-fixtures-build.sh`, `zephyr-fixture-leaves.sh`,
`zephyr-fixture-run-one.sh`). Gate `check-fixture-interfaces-opt-in` refuses a
`scripts/build/*fixture*.sh` that configures (`cmake … -S/-B`, `west`, a
`… configure`) without calling it; it self-tests its matcher, and removing the
call from any of the six turns it red.

Measured after the fix, same host, same command: both rows build
(`compile-check fixtures built: 2 row(s)`).

## Left for a decision

Two sessions' rules disagree. Issue 2278 assumes CI sets AMENT_PREFIX_PATH;
the runner image refuses to. This fix keeps both (fixtures opt in, users still
refuse). The other resolution is for the image to set AMENT_PREFIX_PATH and
install every interface package the fixtures use (`diagnostic_msgs` is not in
`[prereq.ros-interface-packages]`). Either way, the choice is the maintainer's.
