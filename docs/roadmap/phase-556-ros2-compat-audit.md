# Phase 556 — ROS 2 interface compatibility audit

**Status (2026-11-13). IN PROGRESS: W0 is done, and a first by-hand
measurement of W2 to W6 found three gaps (W10 to W12) and one question for
the user (W13).**
The user's goal is to "ensure the interface compatibility among nano-ros and
stock ROS 2". This phase measures it: a nano-ros node and a stock `rclcpp`
node that declare the same interface are asked the same `ros2` CLI questions,
and the two answers are compared. Every difference that is not by design
becomes an issue with a measured reproduction and a work item here.

Each item is claimed with `just claim phase-556-wN` before it starts.

Every message an item adds that a user can see (a test's failure text, a CLI
line) names the interface, the node or the command, and carries no issue,
phase or RFC id (user ruling 2026-11-12). Ids go in source comments.

---

## How a gap is tested

Per the ruling relayed on 2026-11-12, a measured gap is held by a
**strict expected-failure** test, never by a skip and never by a red test in a
merge-gating lane:

- the test asserts that the measured difference is still there, and its
  source comment names the issue that tracks it, so CI stays green while the
  gap is real;
- the test FAILS the day the gap is fixed, which forces whoever fixed it to
  flip the assertion to the plain compatibility check.

A gap's work item is DONE when its test has been flipped to the plain check
and passes. Where the test helpers cannot express the expected-failure form,
the gap uses `nros_tests::unmet!` naming the gap in a lane that gates no
merge, and the item says so.

## Scope and what it cannot cover

- The pair runs on native Linux over zenoh: nano-ros through zenoh-pico, the
  stock node through `rmw_zenoh_cpp`, one `rmw_zenohd` between them. The
  queries use `nros_tests::ros2::ros2_query_cmd` (no daemon).
- The box has Humble only. Jazzy is reached through the docker edition axis
  (`ros_editions_e2e`). W8 records what docker cannot show.
- Lifecycle is cross-referenced, not re-measured: phase-554 owns it.
- DDS (Cyclone) and XRCE pairs are out of scope for the first pass; W9
  decides whether they need their own.

## Work items

### W0 — Inventory the interop tests that exist

- **Status:** DONE (2026-11-13, this doc)

Read the interop tests under `packages/testing/nros-tests/tests/`
(`graph_interop`, `params_per_node_interop`, `c_param_server_interop`,
`qos_event_interop`, `ros2_action_e2e`, `rosout_interop`,
`ros_editions_nano_interop`, `advertised_state_interop`, `interop_e2e`,
`xrce_ros2_interop`) and their `interop::CELLS` rows. For each axis below,
record which test already compares nano-ros with stock ROS 2, which only
checks that nano-ros answers, and which axis has no test.

Acceptance: a table in this doc, one row per axis, naming the test (or none)
and what it compares.

Result: **no test asks a stock node and a nano-ros node the same question
and compares the answers.** The two nearest are
`advertised_state_interop.rs` (Cyclone only: what a stock peer reads about
our QoS, against our own actual-QoS slot) and `graph_interop.rs` (the GID a
stock peer reads, against our own). Paths are under
`packages/testing/nros-tests/tests/`.

| axis | checks only that nano-ros answers | no test |
|---|---|---|
| param list, get, set | `params.rs` (string matches), `params_per_node_interop.rs` (zenoh, cyclone, xrce), `c_param_server_interop.rs` | |
| param describe | `params.rs`, a loose match | |
| param dump | | yes |
| the six parameter services | `qos_override_e2e.rs` (names, no types) | types |
| `service list -t` | `ros2_action_e2e.rs` (waits for one hidden service) | `-t` output |
| `action list -t` | `ros2_action_e2e.rs` drives goals, never lists | listing |
| `node list` | `graph_interop.rs`, `rust_multi_node_per_node_graph.rs` | |
| `node info` | `rust_multi_node_per_node_graph.rs` (XRCE only, one section) | zenoh |
| `topic list -t` | a helper in `src/ros2.rs` that no test calls | yes |
| `/rosout`, `/parameter_events` | `rosout_interop.rs`, `params_per_node_interop.rs`, `c_param_server_interop.rs` (echo the content) | presence in the graph |
| `topic info -v` QoS | `workspace_features_e2e.rs`, `qos_override_e2e.rs` (against the declared profile); `graph_interop.rs`, `xrce_ros2_interop.rs` (GID) | |
| lifecycle | `interop_e2e.rs`, `workspace_features_e2e.rs`, `entry_e2e.rs` (stock `ros2 lifecycle` drives a nano-ros node) | phase-554 owns it (W7) |

### First measurement of W2 to W6 (2026-11-13, by hand)

Humble with `rmw_zenoh_cpp`, `--no-daemon`. nano-ros: fixture
`param-two-node-talker` and the native Rust talker, listener, service-server
and action-server. Stock: an rclpy node with the same parameters,
`demo_nodes_cpp` talker, listener and `add_two_ints_server`, and
`examples_rclcpp_minimal_action_server`.

Matches stock: `param list`, `get`, `set` and `--param-type` on declared
parameters; `param describe` on a declared parameter, byte for byte; the six
parameter service names and types on the first node; `service list -t`;
`action list -t` and `action info -t`; the hidden action services and topics
with their types; the `node info` action section; the QoS of
`/parameter_events`, `_action/status` and `_action/feedback`; reliability,
durability, lifespan, deadline and liveliness of a default-QoS topic.

Differs: W10 to W13 below. The `/chatter` history depth differs because the
examples declare different depths, which is not a gap.

Not measured yet: non-default deadline, lifespan and liveliness (W6); C and
C++ nano-ros nodes (W2); `/parameter_events` message content and
`ros2 param load` (W2); Jazzy (W8).

### W1 — A paired-node comparison harness

- **Status:** OPEN

One helper that starts a nano-ros node and a stock `rclcpp` node declaring the
same interface on one domain, runs one `ros2` query against each, normalises
the node-name and ordering differences that are by design, and returns the
difference. Each axis is a cell in `interop::CELLS`, not a hand-picked
fixture.

Acceptance: the helper compares a pair whose answers are known to match and
reports no difference, and reports a seeded difference.

### W2 — Parameter services

- **Status:** OPEN

`ros2 param list`, `get`, `set`, `describe` and `dump` against both nodes:
the six parameter service names and types, value types, descriptors, and the
answers to a get or set of an undeclared name.

Acceptance: one test per query, plain or strict expected-failure.

### W3 — Service and action names and types

- **Status:** OPEN

`ros2 service list -t` and `ros2 action list -t` for a node with one service
server and one action server: the names, the types, and the hidden services
an action brings.

Acceptance: as W2.

### W4 — `ros2 node info`

- **Status:** OPEN

Publishers, subscribers, service servers and clients, action servers and
clients, as `ros2 node info` lists them for each node.

Acceptance: as W2.

### W5 — `ros2 topic list -t`

- **Status:** OPEN

Topic names and types, including the topics every stock node adds by itself
(`/rosout`, `/parameter_events`).

Acceptance: as W2.

### W6 — QoS as the graph reports it

- **Status:** OPEN

`ros2 topic info -v` for a publisher and a subscription with non-default
QoS: reliability, durability, history, depth, deadline, lifespan, liveliness.

Acceptance: as W2.

### W7 — Lifecycle cross-reference

- **Status:** OPEN

No new measurement. Record which of phase-554's items answer the lifecycle
part of the user's goal, so this audit's result table is complete.

Acceptance: a row in the W0 table pointing at the phase-554 items.

### W8 — Jazzy type hashes through the docker edition axis

- **Status:** OPEN

Run the W2 to W6 queries that the docker edition axis can carry against a
Jazzy peer, with attention to the RIHS01 type hashes Jazzy advertises. Record
what the docker setup cannot show that a Jazzy host would.

Acceptance: the docker-carried queries are tests; the rest is a written list
in this item.

### W9 — Decide the DDS and XRCE passes

- **Status:** OPEN

From W2 to W6's results, decide whether the Cyclone and XRCE pairs need the
same queries, and add their items here if so.

Acceptance: a decision with its reason, in this item.

## Gap items

Each gap W2 to W8 measure is added here as its own `### W<n>` item, with its
issue, its expected-failure test, and the acceptance "the test is flipped to
the plain check and passes".

### W10 — The second node of an image misses a parameter service

- **Status:** OPEN

[Issue 2482](../issues/2482-second-node-param-service-missing.md): in a
two-node zenoh image the second node serves 5 of the 6 parameter services
(`get_parameter_types` is missing) and publishes no `/parameter_events`.

Acceptance: a test asks `ros2 node info` for each node of a two-node image,
held in expected-failure form until the fix, then flipped to the plain check
and passing.

### W11 — `use_sim_time` is not declared

- **Status:** OPEN

[Issue 2483](../issues/2483-use-sim-time-not-declared.md): every stock node
declares `use_sim_time`; a nano-ros node with parameter services does not, so
`ros2 param set <node> use_sim_time true` is refused.

Acceptance: as W10, comparing `param list` and `param get use_sim_time`.

### W12 — Replies for undeclared names, and refusal reasons

- **Status:** OPEN

[Issue 2484](../issues/2484-param-service-undeclared-replies.md):
`GetParameters` and `DescribeParameters` answer an undeclared name with a
filler entry where stock answers with none, so `ros2 param get` exits 0; and
the delete and off-step refusals give a different reason from stock.

Acceptance: as W10, one test per reply shape and refusal reason.

### W13 — Which interfaces a node has by default

- **Status:** BLOCKED (user ruling)

A stock node always serves the six parameter services, publishes `/rosout`
and `/parameter_events`, and subscribes to `/parameter_events`. A nano-ros
node by default has none of these: RFC-0060 says nano-ros "defaults them
off", and `/rosout` is opt-in through `nros-log`. The cost a user sees:
`ros2 param list /talker` prints nothing with exit 0, and
`ros2 param get /talker x` fails with `Wait for service timed out`.

Question for the user: keep the defaults off (each service is a zenoh
queryable and costs RAM on small targets) and document the difference, or
turn them on where the board can afford it, for example on native Linux.

Acceptance: the ruling is recorded here, and either the defaults move with a
test comparing `node info` against stock, or the difference is documented
where a user meets it.
