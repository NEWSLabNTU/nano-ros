# Phase 488 — one larger real-world ROS 2 C++ node, ported unmodified

**Status (2026-10-11). Opened.** Carries phase-209 G.4, which
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

## Work items

### W1 — pick the node

A real, published ROS 2 C++ node, ported from its upstream source, that uses:

- more than two message types;
- a service (server or client);
- at least one parameter.

[`docs/research/autoware-port-survey.md`](../research/autoware-port-survey.md)
names `autoware_external_cmd_selector` (316 SLOC, a command-source state
machine) as close to that shape; check it against the three requirements and
against what its message packages pull in before choosing it. Record the pick,
the upstream commit, and its license here.

### W2 — port it

- Generate its message packages with `nros generate cpp`.
- Apply only the RFC-0096 D5 edits. Every further edit the node needs is a
  finding: either nano-ros grows the missing surface, or the edit joins D5 as a
  stated one, and the RFC records which.
- Add it as a port template under `examples/templates/`.

### W3 — run it everywhere

- Runtime cells on posix, Zephyr (mps2/an385) and FreeRTOS (mps2-an385), as
  `matrix::CELLS` rows run by `port_templates_e2e`, checked from outside by
  host peers on the same router, like the existing three.
- The cells exercise the service and the parameter, not only the topics.

## Acceptance

- The node builds and runs on posix, Zephyr and FreeRTOS with only the D5
  edits, or with each extra edit recorded in RFC-0096.
- Each gap it found is fixed or filed as an issue that names this phase.
