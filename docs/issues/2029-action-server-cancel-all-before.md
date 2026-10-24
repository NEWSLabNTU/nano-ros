---
id: 2029
title: "A nano-ros action server ignores the zero-id CancelGoal forms: cancel-all and cancel-before answer UNKNOWN_GOAL_ID"
status: open
type: enhancement
area: [api, rmw]
severity: low
found: 2026-10-24
related: [2027, phase-515]
---

**Home: [phase-515](../roadmap/phase-515-api-stock-shape-review.md)** — homed 2026-10-24; listed in that phase's table of open API issues.

## What

`action_msgs/srv/CancelGoal` reads its request's `GoalInfo` as a pair: a zero
goal id with a zero stamp cancels EVERY goal, a zero id with a stamp cancels
every goal accepted at or before it, a non-zero id cancels that goal. Issue
2027 gave the C++ client upstream's `async_cancel_all_goals` and
`async_cancel_goals_before(stamp)`, which send exactly those requests
(`ActionClientCore::send_cancel_request_stamped`).

The SERVER side reads only the goal id: `ActionServerCore::try_recv_cancel_request`
(`packages/core/nros-node/src/executor/action_core.rs`) decodes the id and
looks it up (`find_goal_status`), so the zero id is an unknown goal and the
reply is `UNKNOWN_GOAL_ID` with no goals canceling. Against an rclcpp server
the two client calls do what upstream's do; against ours they cancel nothing.

## Why it is not a one-liner

Cancel-all needs the cancel callback asked once per active goal and one reply
listing every goal that moved to CANCELING. Cancel-before also needs each
goal's ACCEPTANCE STAMP, which the arena does not keep: time is a board
capability (RFC-0036) and the accept path has no clock (`c:action_goal_info_t`
in the parity ledger). Cancel-all alone does not need a clock.

## Where it is written

The ledger row `cpp:Client::async_cancel_goals_before` and the porting guide
(`book/src/getting-started/porting-a-cpp-node.md`) both name this issue.
