# Phase 518 — the phase-488 C++ ports on Zephyr and FreeRTOS, and their three open issues

**Status (2026-10-26). Done; W1–W5 done.** Every node builds and runs on
both boards, and a ported workspace is sized from its own program. W1–W3 take the three issues phase-488 left open.
W4 and W5 attempt the three ported nodes on Zephyr and FreeRTOS, which
phase-488 did not try.

**Opened 2026-10-26.** Follows
[phase-488](phase-488-larger-cpp-port-fixture.md), which ported three
real-world ROS 2 C++ nodes to nano-ros on Linux and archived with three issues
open and the RTOS targets unattempted. Implements no new RFC decision. It
measures [RFC-0096](../../design/0096-cpp-freestanding-core-and-porting-layer.md)
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

### W1 — `get_parameter_overrides()` ([issue 2057](../../issues/archived/2057-cpp-parameter-overrides-listing.md))

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

### W2 — a per-request client callback ([issue 2055](../../issues/archived/2055-cpp-client-per-request-callback.md))

`Client<S>::SharedPtr` holds what `create_client<S>(name)` returns, and
`async_send_request(request, callback)` runs the callback from the executor.

**Done when** the composition client compiles unchanged on a hosted build, and
a stub-backend runtime test stages a reply and asserts the callback ran with it.

**Done (2026-10-26).** On a hosted build `ClientHandle<S>` (what
`Client<S>::SharedPtr` names) can hold the future-style client, still in two
words, and `client_->verb(...)` works on it. `async_send_request(request,
callback)` runs `callback(Client<S>::SharedFuture)` from the executor, on a
1 ms timer the client registers and cancels while no callback waits. Inside a
callback, `wait_for_service` no longer starts a second dispatch pass. Upstream's
`client_component.cpp`, unmodified, prints `Got result: [5]` against a stock
ROS 2 server. The pump found a timer bug (a `Skip` timer reset by its own
callback underflowed). Test: `client_per_request_callback_runtime.cpp`. Issue
2055 resolved. On an RTOS toolchain there is no `std::promise`, so the port
keeps the edit (porting guide). The composition's in-process client never sees
its in-process server on posix ([issue 2083](../../issues/2083-posix-in-process-server-not-in-graph.md)).

### W3 — pool sizes derived on the ament road ([issue 2043](../../issues/archived/2043-ament-port-pool-sizing-census.md))

The maintainer ruled (2026-10-24) that the sizes come from the program, as
issue 1556's census does for a standalone leaf. On Linux the image can census
itself after the link. On an RTOS it cannot, so W3 also has to say where a
cross-built image's census comes from (a host build of the same packages is
the obvious candidate).

**Done when** porting node 1 takes no sizing knob on Linux, a `just check` lane
fails if an in-tree ament package's pool is left at its default, and the RTOS
answer is either built or written down in W4's findings.

**Done (2026-10-26).** A workspace states `[[component]] entities = "census"`
in its `system.toml`, and the configure takes the census itself: it builds the
same sources for the host, runs them once, and sizes every pool from what was
created ([issue 2043](../../issues/archived/2043-ament-port-pool-sizing-census.md),
fix section). The census run gets the node's launch parameters from
`params_files` ([issue 2073](../../issues/archived/2073-ported-board-exe-no-params-source.md)).

- Linux: node 1 takes no sizing knob. Its pools come from the census
  (`MAX_CBS = 21`, 12 subscribers, 9 publishers, 28 liveliness tokens), and it
  runs against a stock Humble peer: topics, the service, parameters and
  diagnostics.
- The lane: `just check ament-census-sizing` builds
  `bins/ament-census-port` (six subscriptions, six publishers, two timers, no
  size stated). It fails if a pool the census answers is left at its default.
  It runs in the merge group.
- RTOS, built: a cross-built image takes its census from a host build of
  the same packages. The FreeRTOS template image built `MAX_CBS = 1` from it,
  and its e2e test passes. Zephyr was not read then; it is since
  [issue 2086](../../issues/archived/2086-zephyr-ported-image-not-census-sized.md).

### W4 — build the three nodes on FreeRTOS and Zephyr

Board targets: `mps2-an385-freertos`, and Zephyr `mps2/an385` (this bench is
aarch64, where Zephyr 3.7's `native_sim` does not build). Each port is built
outside the tree, as in phase-488 W2, and each finding becomes a fix with an
in-tree test, a D5 statement, or an issue.

**Done when** each node either builds for both boards or has an issue naming
what stops it.

**Progress (2026-10-26).**

| node | FreeRTOS `mps2-an385-freertos` | Zephyr `mps2_an385` |
| --- | --- | --- |
| 1 `autoware_external_cmd_selector` | builds | builds |
| 2 `nav2_velocity_smoother` | builds | builds |
| 3 `composition` | builds | builds |

What the road needed, all landed:

- **A stock ament workspace builds for a board.** It takes the same
  `add_subdirectory(<nano-ros>)` road the host uses. On a board,
  `add_executable` becomes an image (`ROS2_MAIN`). Ported C++ is built hosted
  (`nros_ported_cxx_hosted`), and message packages that `find_package` only
  validated are generated when a C++ target asks for them. App links go
  through `nros_link_app_dependency`, so a stock plain-signature
  `target_link_libraries(exe lib)` does not trip cmake's signature check.
  Road B reads the leaf's board, locator and domain.
- **Zephyr.** A source's directory properties follow it into `app`, and a
  generated message target carries its include directories.
- **A component executable links on a board.** The generated `main` from
  `rclcpp_components_register_node(... EXECUTABLE ...)` had C linkage.
- `service_is_ready()` passes `int32_t*`, and three board C files lost their
  `-Werror` warnings (Autoware builds `-Werror`).
- `nros build` configures a board image `Release` (issue 2072). It used to
  compile the image unoptimised.

The in-tree regression is the new template
[`examples/templates/ament-workspace-port`](../../../examples/templates/ament-workspace-port/):
a library plus an executable linked to it in a stock package, built for posix,
FreeRTOS and Zephyr. Matrix workload `PortWorkspace`, three cells; e2e
`port_templates_e2e::cpp_port_ament_workspace_publishes{,_on_freertos,_on_zephyr}`.

Node 1 on FreeRTOS needed porter edits that are not committed (the port is the
user's work): parameter defaults (N1-F3, below), a `throw` replaced under
`-fno-exceptions` (N1-F1), an `nros-codegen.toml` capping `diagnostic_msgs` to
4-element sequences and 64-byte strings (N1-F4), the heap at 2048 KiB (N1-F2),
and four sizing knobs that are issue 2043's (recorded there).

Zephyr needs the QEMU fragment `cmake/zephyr/mps2-an385.conf`, which only the
fixture script appends today (issue 1782).

Nodes 1 on Zephyr and 2 on both boards found these, all fixed:

- `ament_target_dependencies` and `rclcpp_components_register_node` scheduled
  the platform link pass on Zephyr, which has none: `Unknown CMake command
  "nros_platform_link_app"` at the end of the directory.
- Autoware's `-Wpedantic -Werror` refused Zephyr's C++ compat headers
  (`#include_next` is a GCC extension) and the empty placeholder TU. The
  headers are `#pragma GCC system_header` now.
- `diagnostic_updater`'s `diagnostic_msgs` were only validated on Zephyr; a
  message package's headers missed their dependencies' include paths and the
  C++ runtime's (`rclcpp/fixed_string.hpp`); `${<msg>_TARGETS}` was empty.
  A Zephyr C++ image and any ported ament package now generate what they find.
- The RTOS toolchains' libstdc++ has no `std::mutex`. nros-cpp supplies
  `std::mutex` / `std::recursive_mutex` over the platform mutex for the
  ported package (`packages/api/nros-cpp/compat/gthreadless/mutex`).
- `nav2_package()` adds `-fPIC` to its directory. On FreeRTOS the GOT fell
  outside the `.data` copy, so every global read as address 0 and the image
  died at "Network init failed"; the shared linker script keeps the GOT in
  flash. On Zephyr, whose linker script discards the GOT, PIC flags no longer
  follow sources into `app`. An edit to the shared linker script did not
  relink anything (issue 0475's class): the `INCLUDE`d script and the
  baremetal and ThreadX `-T` scripts are link dependencies now.
- A package depending on `rclcpp_lifecycle` got no lifecycle runtime on a
  board, so it did not link. The cmake road gives it `lifecycle`; a Zephyr
  application's `[system] features` now reach Kconfig and the runtime.
- On Zephyr a library package of the workspace resolved to the installed
  upstream copy (`nros_workspace_interfaces()` was undefined there, and the
  Find stubs were emitted only when the search path was set early enough).
- The C++ FreeRTOS app stack was a literal 64 KiB. It is
  `NROS_FREERTOS_APP_STACK_KB` now (issue 2070).
- A library package that finds only message packages kept `-ffreestanding`.
  `find_package(ament_cmake)` makes the directory hosted.

The template now has these shapes too: a second, library package found by its
sibling, `-Wpedantic -Werror -fPIC`, a `std::mutex` member, a
`std_msgs/Header` field and a `${std_msgs_TARGETS}` link. Its e2e tests pass on
all three platforms.

Porter edits node 2 needed on both boards: `throw` and `try`/`catch` guarded
with `__cpp_exceptions` (N2-F2), `setSoftRealTimePriority()` a no-op off Linux
(N2-F1), and a 192 KiB app stack on FreeRTOS (N2-F3, issue 2070). Node 1 on
Zephyr needed the same capacities as on FreeRTOS plus a 256 KiB nros heap
(issue 2043).

### W5 — run them under QEMU against stock ROS 2

**Done when** each node that W4 built is driven from a stock ROS 2 Humble peer
under QEMU, by the same checks phase-488 ran on Linux, or has an issue naming
what stops it.

**Progress (2026-10-26).** Driven from ROS 2 Humble (`rmw_zenoh_cpp`) under
QEMU mps2-an385:

- **Node 3 on FreeRTOS and Zephyr:** the talker reaches `ros2 topic echo`,
  `ros2 service call /add_two_ints` is answered, and the listener hears a
  `ros2 topic pub`. On Zephyr the in-image client reached the in-image server
  only after `local_queryable = 1` was stated by hand (issue 2043).
- **Node 1 on FreeRTOS:** `ros2 node info` lists all 12 subscriptions, 8
  publishers and the service. `current_selector_mode` is echoed, and
  `ros2 service call .../select_external_command` answers `success=True` and
  moves the mode from 1 to 2. Boot report: `app task stack peak 13768 of
  65536 bytes`, `heap peak 179976 of 2097152 bytes`.
- **Node 1 on Zephyr:** the same checks pass: the mode is echoed as 1, the
  service answers `success=True`, and the mode reads 2 afterwards.
- **Node 2 on FreeRTOS and Zephyr:** `ros2 lifecycle get` answers
  `unconfigured`, `configure` and `activate` succeed and it reads `active`,
  and a `ros2 topic pub /cmd_vel` comes back on `/cmd_vel_smoothed`
  (`linear.x: 0.125`). FreeRTOS boot report: `app task stack peak 100416 of
  196608 bytes`, `heap peak 311008 of 2097152 bytes`. The parameter services
  are off on a board unless declared, so `ros2 param` was not driven.

Node 1 found two runtime bugs before that:

- [Issue 2071](../../issues/archived/2071-ros-time-frozen-without-rtc.md), fixed:
  with no RTC, ROS time never advanced, so the node's
  `rclcpp::create_timer(this, get_clock(), ...)` never fired.
- [Issue 2070](../../issues/2070-param-declare-stack-cost.md), open: declaring one
  parameter at the default capacities costs ~70 KiB of stack, and the C++
  FreeRTOS app stack is 64 KiB. The overflow corrupted the kernel's globals
  without any stack-overflow report.

A ported executable on a board had no way to receive `--ros-args -p` or
`--params-file`: `argv` is null
([issue 2073](../../issues/archived/2073-ported-board-exe-no-params-source.md), N1-F3). Node 1's two
`declare_parameter<T>(name)` calls with no default got porter-written defaults.
Resolved: the image states `[image.<id>] params_files` / `ros_args` in
`system.toml`, which the C++ runtime bakes and `rclcpp::init` applies.

## Acceptance

- [x] Issues 2043, 2055 and 2057 are resolved, or closed `wontfix` with the
      porting guide stating the edit instead. (All three resolved; 2043's
      Zephyr half continues as issue 2086.)
- [x] Each node has an RTOS build verdict per board, with every finding fixed or
      filed.
- [x] The porting guide states what the RTOS attempt learned.
