---
id: 1239
title: "The `find_package(rclcpp)` compat entry point is not equivalent to the
  `ament_auto_*` one — it force-includes a header phase-417 deleted, and it
  never asks for the std surface phase-438 W2 made a request"
status: resolved
type: bug
area: build, integrations
related: [phase-417, phase-438, rfc-0089, 1187]
resolved_in: "phase-438 W4"
---

## Problem

nano-ros's rclcpp compat layer has TWO entry points that are supposed to be
equivalent: `cmake/compat/NrosRclcppCompat.cmake`'s `ament_auto_*` shims, which
call `_nros_compat_apply_force_includes(<target>)` per target, and
`cmake/compat/stubs/Findrclcpp.cmake`, which serves the upstream-style
`find_package(rclcpp)` + `ament_target_dependencies(<target> rclcpp …)` shape by
publishing everything on the `rclcpp::rclcpp` IMPORTED INTERFACE target.

They drifted apart in both directions, and each drift is a separate missed
sweep.

### Defect 1 — a force-include of a deleted header

`cmake/compat/stubs/Findrclcpp.cmake:24` puts a force-include of
`nros/rclcpp_compat.hpp` on the `rclcpp::rclcpp` IMPORTED target:

```cmake
target_compile_options(rclcpp::rclcpp INTERFACE
    "$<$<COMPILE_LANGUAGE:CXX>:SHELL:-include nros/rclcpp_compat.hpp>"
    "$<$<COMPILE_LANGUAGE:CXX>:SHELL:-include nros/rclcpp_components_compat.hpp>"
)
```

That header no longer exists. `f35f0b878` ("feat(phase-417 stage 6 step A): the
ROS 2 spellings are ours; rclcpp_compat.hpp is DELETED") removed it, moving
`rclcpp::init` / `shutdown` / `ok` / `Node` / `spin` / `Rate` /
`spin_until_future_complete` into `<nros/nros.hpp>` itself. The sibling
force-include of `nros/rclcpp_components_compat.hpp` is fine — that header is
still there, and `cmake/compat/NrosRclcppCompat.cmake:103` force-includes it
and nothing else, which is why the OTHER compat entry point was unaffected.

The stub was last touched in `6fdec829a`, which is an ancestor of `f35f0b878`,
so this is a missed site in the deletion sweep rather than a regression added
afterwards.

### Symptom

Every C++ TU compiled against the stub dies before reading a line of its own
source:

```
<command-line>: fatal error: nros/rclcpp_compat.hpp: No such file or directory
compilation terminated.
```

Measured on `cpp_port_minimal_publisher`, the `[[compile_check_fixture]]` row
whose whole job is to prove an upstream-shaped port compiles.

### Defect 2 — the stub never asks for the std surface

phase-438 W2 made the std-flavoured `rclcpp::` surface reachable only through
`NROS_CPP_STD`, and added `target_compile_definitions(${target} PRIVATE
NROS_CPP_STD=1)` to `_nros_compat_apply_force_includes`. It did not add the
equivalent to this stub — and `ament_target_dependencies` links
`rclcpp::rclcpp` WITHOUT calling that helper, so a consumer written the
upstream way got the include dir and the force-include but not the definition.

Measured on `shadowing` (`examples/templates/workspace-shadowing/src/consumer`,
a stock `find_package(rclcpp)` + `ament_target_dependencies` package), at the
phase-438 W2 commit `d301bd7a7` with the W4 header changes stashed:

```
consumer.cpp:26:44: error: expected class-name before '{' token      // : public rclcpp::Node
consumer.cpp:41:29: error: 'TimerBase' is not a member of 'rclcpp'
consumer.cpp:47:13: error: 'spin' is not a member of 'rclcpp'
```

## Why it went unnoticed

The affected rows are `compile_check_fixture`s, built by
`scripts/build/compile-check-fixtures.sh` in the BUILD stage of a tier lane —
not by `just check fast` and not by the pull-request `CI` context. A red there
is only visible to someone who runs the fixture build, and the CLAUDE.md
"uniformly red lane" note applies: once it fails for one reason, a second cause
behind it is invisible.

`NrosRclcppCompat.cmake` is the entry point the `ament_auto_*` shims use and it
was never broken, so the two paths that are supposed to be equivalent were not.

## Fix

Both halves, in `cmake/compat/stubs/Findrclcpp.cmake`: delete the dead
force-include, and add `target_compile_definitions(rclcpp::rclcpp INTERFACE
NROS_CPP_STD=1)`. INTERFACE rather than PRIVATE because the flag has to reach
whatever links the target, which is the only mechanism the stub has.

Deleting the force-include loses nothing: the names it used to supply are
declared by `<nros/nros.hpp>`, which `cmake/compat/include/rclcpp/rclcpp.hpp`
already includes, and it had been supplying an empty forwarder even before the
file was removed (`cmake/compat/include/rclcpp/rclcpp.hpp:8` still describes it
as one).

After the fix all five ported `cmake-fixture` rows build:
`cpp_port_minimal_publisher`, `cpp_port_rclcpp_compat_smoke`,
`cpp_port_topic_state_monitor`, `local_msg_pkg`, `shadowing`.

Landed with phase-438 W4, which is where it surfaced: verifying that the ported
templates still build against the merged `rclcpp::Node` required these fixtures
to build at all, and they could not.
