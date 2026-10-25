# Phase 488 — real-world ROS 2 C++ nodes, ported, and the friction fixed

**Status (2026-10-25). W1 done: three nodes picked from a survey of nine
projects. W2 and W3 done for node 1 (`autoware_external_cmd_selector`): it
builds with no source edit and runs against a stock ROS 2 peer, with its pool
sizes still raised by hand (issue 2043). Nodes 2 and 3 and W4 not started.** Rescoped 2026-10-24. The ported code is the
user's work and is not committed here. What this phase lands is what the ports
expose: nros-cpp, codegen and build fixes, issues, and porting-guide changes.
The original W2 ("add it as a port template") and W3 (runtime cells for it)
are replaced below.

**Opened 2026-10-11.** Carries phase-209 G.4, which
[phase-482](archived/phase-482-rclcpp-drop-in-residue.md) W3 inherited and did
not attempt. Phase-482 was archived without it, because its acceptance line
("every port template has a runtime cell on posix, Zephyr and FreeRTOS") holds
for the three templates it has. Implements no new RFC decision: it measures
[RFC-0096](../design/0096-cpp-freestanding-core-and-porting-layer.md) D5 (the
`main` and `CMakeLists.txt` edits a port is allowed) against a node bigger than
the ones it was written from.

## Why

The three port templates are deliberately small:

| template | what it uses |
| --- | --- |
| `cpp-port-minimal-publisher` | one publisher, one timer |
| `rclcpp-compat-smoke` | a publisher plus the diagnostics helper |
| `topic-state-monitor-port` | two subscriptions plus the diagnostics helper |

None uses a service, a parameter or more than two message types, so the claim
"a ported rclcpp node needs only the D5 edits" has been measured only on nodes
that never reach those surfaces. Phase-209 G.4 asked for one node that does,
to find the next round of compatibility gaps (composition, several nodes in
one process, intra-process delivery) before a user does.

## W0 — the stock rclcpp examples, measured (2026-10-21, landed)

Before porting one large node, the 13 stock `ros2/examples` rclcpp files
(humble + jazzy: publisher, subscriber, timer, service, client, action
server/client, multithreaded executor) were ported with the fewest edits and
committed beside the verbatim originals in
`packages/api/nros-cpp/tests/compile/rclcpp-examples/`. `just check cpp`
compiles, links and boots every port on the stub backend and fails when an
example's edited-line count (`edits.json`) moves either way.

Measured 254 edited lines before the drop-ins, 226 after. Drop-ins:
`Node::make_shared` / `make_unique` (hosted), `Executor::spin_some` (the
`cpp:Executor::spin_some` wave, one drain with Rust's), the `shared_ptr`
service handler lent from the dispatch frame (overrides the phase-417 W2.c
refusal; no heap, retention aborts), `executors::SingleThreadedExecutor` plus a
refusing `MultiThreadedExecutor`, `std::chrono` overloads of
`wait_for_service` / `wait_for_action_server`, and `<rclcpp_action/rclcpp_action.hpp>`, plus a fix to
`spin_until_future_complete`, which could not take a real `rclcpp::Future`.
What still costs edits is listed per example in `edits.json` and in the
porting guide; the action shapes (164 of the 226 lines) dominate. Filed from
it: issue 1995 (pointer-shaped subscription callbacks) and issue 1996 (the
client future shape).

### W0 follow-up (2026-10-25)

**Landed since W0.** Each was filed from the ported examples or from the API
parity work around them:

| issue | PR | what |
| --- | --- | --- |
| [1995](../issues/archived/1995-cpp-pointer-shaped-subscription-callbacks.md) | #2513 | pointer-shaped subscription callbacks (hosted) |
| [1996](../issues/archived/1996-cpp-client-shared-future-shape.md) | #2524 | the client future's `get` / `wait_for` / `valid` |
| [2009](../issues/archived/2009-cpp-client-sequence-correlation.md) | #2533 | replies matched to requests by sequence id, every client |
| [2011](../issues/archived/2011-cpp-action-client-future-global-stash.md) | #2540 | C++ action futures keyed per goal |
| [2014](../issues/archived/2014-polling-action-client-result-offset.md) | #2552 | the get_result payload offset, spelled once |
| [2015](../issues/archived/2015-rclcpp-action-upstream-dropins.md) | #2561 | upstream `rclcpp_action` spellings as drop-ins |
| [2023](../issues/archived/2023-cpp-rust-parameter-events.md) | #2575, #2581 | C++ and Rust publish `/parameter_events` |
| [2027](../issues/archived/2027-action-goal-handle-api.md) | #2587 | handle-taking action calls, `ServerGoalHandle::execute` |
| [2033](../issues/archived/2033-typed-client-helpers-drop-seq.md) | #2595 | the typed Rust client helpers carry the sequence id |
| [2034](../issues/archived/2034-cpp-executor-cancel-reply-callback.md) | (2026-10-25) | the executor's spin takes C++ cancel replies |
| [2037](../issues/archived/2037-cpp-client-future-and-request-id.md) | (2026-10-25) | `FutureAndRequestId`: `.future`, `.request_id`, `share()` |
| [2024](../issues/archived/2024-parameter-event-handler.md) | (2026-10-25) | `rclcpp::ParameterEventHandler` (hosted) |
| [2035](../issues/archived/2035-cpp-node-options-param-event-publisher-off.md) | (2026-10-25) | `start_parameter_event_publisher(false)` honoured |

#2612 (in the merge queue on this date) closes the two action-cancel issues,
#2029 (cancel-all / cancel-before on the server) and #2030 (the C tier takes
cancel replies), and files #2034.

**Edited lines, as measured** (`edits.json`, `edit_count.py --report`): 254
before the drop-ins → 226 at W0 → 214 after 1995 → 199 after 1996 → **43**
after 2015. The other issues in the table moved no example's count.

**What the 43 lines are**, per `edits.json`'s `remaining`:

- `multithreaded_executor` (18). `MultiThreadedExecutor` is refused (RFC-0002:
  one task per executor, no hidden threads), and callback groups have no
  counterpart, so the group plumbing is deleted.
- `minimal_publisher_not_composable` (11). The try/catch around publish and
  spin is removed: no exceptions (RFC-0018).
- `minimal_action_server_member_functions` (6). The stock `handle_accepted`
  detaches a `std::thread` per goal. The port runs `execute(goal_handle)` in
  the accepted callback and drops `rclcpp::Rate::sleep`.
- `minimal_subscriber_not_composable` and `minimal_service` (3 each). A global
  `Node::SharedPtr` cannot be assigned from a temporary, because ours does not
  own.
- `minimal_action_client_member_functions` (2). A generated sequence has no
  `.back()`, so `seq.back()` is spelled `seq[seq.length() - 1]`.

**Open issues the stock examples or the parity work still point at:**

- [issue 2036](../issues/2036-cpp-subscription-message-info-callback.md): no
  `rclcpp::MessageInfo`, so the `(const MsgT&, const MessageInfo&)` callback
  does not compile.
- [issue 2038](../issues/2038-declarative-timer-autostart-and-info.md): the
  declarative Rust timer has no autostart flag or call info.
- [issue 2039](../issues/2039-c-api-matched-status-events-interop.md): no
  interop cell drives the C matched and status-event callbacks from stock ROS 2.

## Work items

### W1 — pick the nodes (done 2026-10-24)

The original requirement was one real, published ROS 2 C++ node with more than
two message types, a service and a parameter. A survey of nine projects found
that one node covers only some of the surfaces worth testing, so the phase
ports three. Each one is chosen to reach a different gap.

**Pins.** The local Autoware checkout is `autoware_universe` at
upstream tag 0.45.0. Nav2 and `ros2/demos` are their `jazzy` branches as of
2026-10-24. All three projects are Apache-2.0.

| # | node | lines | what it uses | gaps it should reach |
| --- | --- | --- | --- | --- |
| 1 | Autoware `autoware_external_cmd_selector` @0.45.0 (`control/`) | 223 | 8 types (pedals, steering, gear, turn and hazard commands, heartbeat, selector mode, `srv/ExternalCommandSelect`); a service server; 2 parameters; 12 subscriptions, 7 publishers | callback groups made with `CallbackGroupType::MutuallyExclusive` and passed through `SubscriptionOptions` (nros takes a name string); a `std::bind` member service handler; `rmw_qos_profile_services_default`; `diagnostic_updater`; generating `autoware_adapi_v1_msgs` and `tier4_control_msgs` |
| 2 | Nav2 `nav2_velocity_smoother` @jazzy | 566, plus the `nav2_util` subset it needs | `Twist`, `TwistStamped`, `Odometry`; lifecycle `ChangeState` / `GetState`; 14 parameters, some of them double arrays | subclassing the lifecycle node through Nav2's own base (`nav2_util::LifecycleNode`); a lifecycle publisher activated in `on_activate`; `rclcpp::ParameterValue`; `declare_parameter_if_not_declared`; a dynamic `add_on_set_parameters_callback`; `bondcpp`; `setSoftRealTimePriority` (Linux scheduling) |
| 3 | `ros2/demos` `composition`, `manual_composition` @jazzy | 401 | `std_msgs/String`, `example_interfaces/srv/AddTwoInts`; a server and a client; four nodes | several nodes on one executor in one process, the item phase-209 G.4 asked for; nros's `add_node` "checks rather than attaches" |

Node 1 is the original pick. It is pinned at 0.45.0 because on upstream `main`
(surveyed 2026-10-24) it is built on `autoware_agnocast_wrapper`, with its own `Node` base
and message-pointer macros. That is a port of agnocast, not of rclcpp.

**Backups.** Each covers one surface the three above do not.

- Autoware `autoware_mrm_emergency_stop_operator` @0.45.0 (158 lines). It uses a
  parameter callback taking `std::vector<rclcpp::Parameter>` and returning
  `SetParametersResult`, which is a different shape from nros's C function
  pointer plus context, and `rclcpp::create_timer` with a clock.
- Autoware `autoware_mrm_handler` and `autoware_joy_controller`. They use
  `autoware_utils`' polling subscriber (`take()`) and client futures awaited
  inside a callback.
- topic_tools `mux`. It uses generic (serialized) publishers and subscribers,
  and entities created at run time. This gap is larger than the others and
  will likely be filed rather than fixed.

**Surveyed and not picked.**

- `autoware_steer_offset_estimator` grew from 212 to 835 lines. It now pulls in
  yaml-cpp, tf2, `fmt` and a file write, which retires the
  `autoware-port-survey.md` ranking.
- teleop_twist_joy and twist_mux have no service.
- The `ros2/demos` lifecycle nodes have no parameter.
- `nav2_lifecycle_manager` uses `std::thread` and a refused
  `MultiThreadedExecutor`.
- `diagnostic_aggregator` uses pluginlib.
- robot_localization and `diff_drive_controller` need Eigen, tf2 and
  controller plugins.
- `topic_state_monitor` already has a port template.

The survey has known limits. Message-package closures were not walked. Humble
branches and Nav2's newer `nav2::LifecycleNode` were not compared. Whether nros
accepts a `std::bind` member as a service handler was inferred from the porting
guide, not tested.

### W2 — port each node, outside the tree

- Copy the upstream package into a scratch workspace. It is not committed to
  nano-ros, because ported code is the user's work.
- Generate its message packages with `nros generate cpp`.
- Apply only the RFC-0096 D5 edits (`main` and `CMakeLists.txt`).
- Log every further edit the node needs, and every workaround, with the file,
  the upstream line and the reason. Each entry is a finding for W3.
- Build and run it on posix and check it from a host peer on the same router.
  Exercise its service and its parameters, not only its topics. Zephyr
  (mps2/an385) and FreeRTOS (mps2-an385) follow where the SDKs are provisioned.

### W3 — turn each finding into a fix or an issue

For each logged edit, choose one:

- nano-ros grows the missing surface (nros-cpp, codegen, the CMake helpers),
  with a test in the tree that does not depend on the ported code;
- the edit becomes a stated D5 edit, recorded in RFC-0096;
- or an issue is filed that names this phase, when the fix is larger than the
  phase.

The stock-example check from W0 (`packages/api/nros-cpp/tests/compile/rclcpp-examples/`,
`edits.json`) is the in-tree regression for a drop-in. Where a finding reduces
to an API shape the stock examples also use, its test goes there.

### W2/W3 progress — node 1, `autoware_external_cmd_selector` @0.45.0 (2026-10-25)

The package was copied verbatim into a scratch workspace whose umbrella
`CMakeLists.txt` adds nano-ros and the package, with the Autoware 0.45.1
install sourced (that is how its message packages and `autoware_cmake` reach a
user's machine). It needed **no source edit and no D5 edit** once the fixes
below landed. It runs on posix against ROS 2 Humble `rmw_zenoh_cpp` on one
router. The peer sees the node and its 19 topics. A
`select_external_command` call switches the mode from 1 to 2 (`success=True`).
`ros2 param get` answers from the params file, `/diagnostics` publishes the
heartbeat, and a remote gear command is forwarded.

| finding | disposition |
| --- | --- |
| `rclcpp_components_register_node(... EXECUTABLE)` failed on every call (`file(GENERATE)` argument error) | fixed, #2620 |
| the generated component main named the class without declaring it | fixed, #2620 (`make_component<T>` factory defined by `RCLCPP_COMPONENTS_REGISTER_NODE`) |
| `ament_auto_find_build_dependencies` was a no-op: no `include/`, no package.xml deps | fixed, #2620 |
| an AMENT message package resolved to the installed ROS package | fixed, #2620 (Find stubs for the AMENT layer) |
| `diagnostic_updater` did not carry `diagnostic_msgs` | fixed, #2620 |
| `ament_auto_add_library(... SHARED)` linked the runtime twice; start-up `undefined symbol` | fixed, #2620 (every kind STATIC) |
| `ros2 param get` timed out: the ament road declared no `param_services` | fixed, #2620 + C++ PR (host ament packages default to it; the node constructor starts them) |
| `<diagnostic_updater/update_functions.hpp>` missing | header added, C++ PR; its classes are issue 2053 |
| `Updater` never published unless `update()` was called (18 of 55 Autoware users never call it) | fixed, C++ PR (own timer) |
| `diagnostic_updater::Updater updater_{this};` | fixed, C++ PR |
| `pub->publish(msg);` an error under `-Wall -Werror` (`[[nodiscard]]`) | fixed, C++ PR: ignorable and logged on the std surface; embedded keeps the warning |
| `CallbackGroupType`, `CallbackGroup::SharedPtr`, `SubscriptionOptions::callback_group` absent | fixed, C++ PR: unnamed groups run on the node default |
| `create_service(name, cb, rmw_qos_profile_services_default, group)` | fixed, C++ PR; the `rmw_qos_profile_t` TYPE is issue 2054 |
| four-argument `create_subscription(topic, qos, cb, options)` | fixed, C++ PR |
| a `std::bind(&X::f, this, _1, _2)` service handler returning `bool` was refused | fixed, C++ PR: a stateful handler's capture lives in the service's arena entry |
| `ExecutorFull`, then liveliness, publisher, subscriber and queryable tables full at start-up | issue 2043 (census sizing on the ament road, as ruled); the executor log now names its knob |
| zenoh warnings advise a retired `ZPICO_*` env spelling | fixed, #2624 (issue 2042) |
| `find_package(autoware_cmake)` needs the project's install sourced | porting-guide note (W4); no fix |

In-tree tests, none of which use the ported code:
`just check cmake-ament-surface` (`tests/cmake-ament-surface-tests.sh`), and
`ament_port_surface_runtime.cpp` in `just check cpp`, built twice (with and
without a `param_services` runtime) plus a link of the generated component
main. Also `a_capturing_raw_service_destroys_and_releases_its_capture` in
nros-node.

### W4 — the porting guide says what the ports learned

Each fixed or stated edit appears in the porting guide, with a before and
after taken from the port. The guide lists the remaining edits per surface, so
the next user knows the cost before they start.

## Acceptance

- Each of the three nodes builds and runs on posix, outside the tree, with only
  the D5 edits plus the logged edits that W3 classified. Zephyr and FreeRTOS
  runs are reported wherever the SDKs allowed them.
- Every logged edit is fixed in nano-ros with an in-tree test, stated in
  RFC-0096, or filed as an issue that names this phase.
- The porting guide reflects the result (W4).
- No ported upstream source is committed.
