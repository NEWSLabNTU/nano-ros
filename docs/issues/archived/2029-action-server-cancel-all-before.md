---
id: 2029
title: "A nano-ros action server ignores the zero-id CancelGoal forms: cancel-all and cancel-before answer UNKNOWN_GOAL_ID"
status: resolved
type: enhancement
area: [api, rmw]
severity: low
found: 2026-10-24
resolved: 2026-10-24
related: [2027, phase-515]
---

**Home: [phase-515](../../roadmap/archived/phase-515-api-stock-shape-review.md)** — homed 2026-10-24; listed in that phase's table of open API issues.

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

## Resolution

Shared core code, so the C, C++ and Rust callback tiers all get it: every
arena action-server dispatch (`action_server_try_process`,
`action_server_raw_try_process`) and the typed `ActionServer::try_handle_cancel`
/ `poll` go through `ActionServerCore::try_handle_cancel`
(`packages/core/nros-node/src/executor/action_core.rs`).

- **The request is read as upstream's pair.** `GoalInfo.stamp` is decoded after
  the id (a stamp-less request from an old nano-ros client reads as zero).
  `select_cancel_goals` applies `rcl_action_process_cancel_request`'s table:
  zero id + zero stamp selects every cancelable goal; zero id + stamp, those
  accepted at or before it; an id + zero stamp, that goal; an id + stamp, that
  goal plus those before the stamp. Cancelable = `Accepted` or `Executing`.
- **The cancel callback is asked once per selected goal** (the handler is
  `FnMut` now, on `ActionServerCore` and the typed `ActionServer`). The goals it
  accepts move to `Canceling` and are listed in `goals_canceling`, each with its
  acceptance stamp. The status array is published once.
- **Return codes, as `rcl_action`:** a non-zero id naming no active goal answers
  `ERROR_UNKNOWN_GOAL_ID` (or `ERROR_GOAL_TERMINATED` while its result is
  retained) without asking the callback; an empty `goals_canceling` is
  `ERROR_REJECTED`; otherwise `ERROR_NONE`. Before, the callback was asked
  about the unknown goal, and an accepting callback produced an `ERROR_NONE`
  reply listing a goal that did not exist.
- **The acceptance stamp.** `RawActiveGoal` gained `stamp`, read from
  `Clock::ros_time()` when the goal is accepted (rcl_action stamps with the
  server's clock the same way). It is written into the SendGoal reply, the
  status array and the polling tier's `send_cancel_reply`, which all wrote zero
  before.

Tests (`packages/core/nros-node/src/executor/tests.rs`, the
`node-std-tests` lane): `a_zero_id_zero_stamp_cancel_cancels_every_cancelable_goal`,
`a_zero_id_stamped_cancel_cancels_the_goals_accepted_at_or_before_it`,
`a_named_stamped_cancel_cancels_that_goal_and_those_before_the_stamp`,
`a_named_unstamped_cancel_cancels_that_goal_alone`,
`the_cancel_callback_decides_per_goal`,
`an_unknown_or_finished_goal_is_answered_without_asking_the_callback`,
`a_cancel_all_with_nothing_to_cancel_is_rejected`. Mutation: treating the
zero id as a goal name (the pre-2029 behaviour) fails four of them.

## Still open

- **A board with no wall clock stamps every goal zero**, so a stamped
  (cancel-before) request there selects every goal. Cancel-all does not need a
  clock.
- **The polling tier still hands the user the request's id alone.**
  `rclc_polling_action_server_try_recv_cancel_request_raw` /
  `nros_cpp_action_server_try_recv_cancel_request_raw` report the goal id the
  request named and its status. For a zero-id request that status is UNKNOWN,
  and the stamp is not reported, so a polling server that wants cancel-all
  applies the rule itself.
- No interop cell against an rclcpp action client's `async_cancel_all_goals`.
  The core is unit-tested over the mock services.
