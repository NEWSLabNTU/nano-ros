# Phase 518 — the phase-488 C++ ports on Zephyr and FreeRTOS, and their three open issues

**Status (2026-10-26). Open; W1 done.** W1–W3 take the three issues
phase-488 left open. W4 and W5 attempt the three ported nodes on Zephyr and
FreeRTOS, which phase-488 did not try.

**Opened 2026-10-26.** Follows
[phase-488](archived/phase-488-larger-cpp-port-fixture.md), which ported three
real-world ROS 2 C++ nodes to nano-ros on Linux and archived with three issues
open and the RTOS targets unattempted. Implements no new RFC decision. It
measures [RFC-0096](../design/0096-cpp-freestanding-core-and-porting-layer.md)
D5 (the edits a port is allowed) on the two RTOS boards a ported program
already reaches through `ROS2_MAIN` (phase-482 W3, phase-499 W5).

## Why

Phase-488's acceptance was "builds and runs against a stock ROS 2 peer" on
Linux. A ported node that only runs on Linux has not shown what nano-ros is
for. The three nodes are bigger than every port template that has an RTOS
cell, so the RTOS attempt is where the sizing issue (2043) stops being a
convenience and becomes the whole question: on a cross-built image no census
can run on the image itself.

The ported code is the user's work and is not committed here (phase-488's
rescoping, 2026-10-24). What this phase lands is what the attempts expose:
nros-cpp, codegen and build fixes, issues, and porting-guide changes.

## The nodes

| node | upstream | Linux status (phase-488) |
| --- | --- | --- |
| 1 | `autoware_external_cmd_selector` @0.45.0 | runs; five pool knobs raised by hand (issue 2043) |
| 2 | Nav2 `nav2_velocity_smoother` + `nav2_util` @jazzy | runs; lifecycle, parameters, `cmd_vel` path |
| 3 | ros2/demos `composition` @jazzy | runs; one source edit (issue 2055) |

## Work items

### W1 — `get_parameter_overrides()` ([issue 2057](../issues/archived/2057-cpp-parameter-overrides-listing.md))

A per-node FFI listing over the executor's argv overrides, typed on the hosted
C++ side into a `std::map<std::string, rclcpp::ParameterValue>`.

**Done when** Nav2's `declare_or_get_parameter` compiles unchanged on a hosted
build, and a runtime test installs `-p a:=1 -p other_node:b:=x` and lists
`{a: 1}` for the first node only.

**Done (2026-10-26).** `Executor::argv_param_overrides_on`, the FFI
`nros_cpp_node_list_param_overrides`, and the hosted
`NodeParametersInterface::get_parameter_overrides()`, which returns a reference
to one copy per node, as upstream's does (Nav2 compares iterators from two
calls).
Tested in nros-node and in `parameter_value_runtime.cpp`. Issue 2057 resolved.

### W2 — a per-request client callback ([issue 2055](../issues/2055-cpp-client-per-request-callback.md))

`Client<S>::SharedPtr` holds what `create_client<S>(name)` returns, and
`async_send_request(request, callback)` runs the callback from the executor.

**Done when** the composition client compiles unchanged on a hosted build, and
a stub-backend runtime test stages a reply and asserts the callback ran with it.

### W3 — pool sizes derived on the ament road ([issue 2043](../issues/2043-ament-port-pool-sizing-census.md))

The maintainer ruled (2026-10-24) that the sizes come from the program, as
issue 1556's census does for a standalone leaf. On Linux the image can census
itself after the link. On an RTOS it cannot, so W3 also has to say where a
cross-built image's census comes from (a host build of the same packages is
the obvious candidate).

**Done when** porting node 1 takes no sizing knob on Linux, a `just check` lane
fails if an in-tree ament package's pool is left at its default, and the RTOS
answer is either built or written down in W4's findings.

### W4 — build the three nodes on FreeRTOS and Zephyr

Board targets: `mps2-an385-freertos`, and Zephyr `mps2/an385` (this bench is
aarch64, where Zephyr 3.7's `native_sim` does not build). Each port is built
outside the tree, as in phase-488 W2, and each finding becomes a fix with an
in-tree test, a D5 statement, or an issue.

**Done when** each node either builds for both boards or has an issue naming
what stops it.

### W5 — run them under QEMU against stock ROS 2

**Done when** each node that W4 built is driven from a stock ROS 2 Humble peer
under QEMU, by the same checks phase-488 ran on Linux, or has an issue naming
what stops it.

## Acceptance

- [ ] Issues 2043, 2055 and 2057 are resolved, or closed `wontfix` with the
      porting guide stating the edit instead.
- [ ] Each node has an RTOS build verdict per board, with every finding fixed or
      filed.
- [ ] The porting guide states what the RTOS attempt learned.
