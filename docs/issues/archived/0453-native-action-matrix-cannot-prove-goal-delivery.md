---
id: 453
title: "No native action cell proved the goal payload was delivered — the example servers ignored or reinvented `order`"
status: resolved  # fixed 2026-08-07
type: bug
area: testing
related: [issue-0448, issue-0450, issue-0461, issue-0467, phase-329, rfc-0051]
---

## The gap

`native_example_reqresp_e2e` asserted, for each action cell, only that the
client logged `ACTION_RESULT_PREFIX` (`"Result received:"`). A client prints
that as soon as it DECODES a result — including the zeroed default it decodes
when the goal never reached the server. So no native action cell could
distinguish "the goal crossed and the server computed" from "the goal was
dropped on the wire".

The service cells never had this gap: they assert `Result of add_two_ints: 5`,
a value the SERVER computed from the request.

## What it cost — two real bugs, one week

Both were green across every native action cell:

- **#0448** — the Rust client shipped TWO CDR encapsulations, so every
  `SendGoal_Request` was 4 bytes over the ROS 2 layout and Fast-DDS dropped it
  outright. Only the XRCE↔ROS 2 interop test caught it, and only because a real
  `rcl_action` server was on the other end.
- **#0461** — the server decoded the goal UUID as `order`. Invisible with a
  nano-ros client, whose UUID begins with a goal COUNTER so `order` always
  looked like a small positive number; it surfaced only against a ROS 2 client,
  whose UUID is random (→ #0467, ~50% of goals rejected).

Same blind spot twice: a nano↔nano test cannot see either, because both sides
share the defect.

## Why it was not fixable when filed

The three example servers shared no convention, and one did not read the goal:

| server | for `order = 10`, when this was filed |
| --- | --- |
| `native/rust/action-server` | fixed `[0, 1, 1]` — `goal.order` destructured as `_order`, never used |
| `native/cpp/action-server` | 10 elements (`i < goal.order`) |
| `native/c/action-server` | 11 elements, but from a hard-coded `int32_t order = 10;` |
| ROS 2 `action_tutorials` | 11 elements (`order + 1`) |

An assertion over a single expected sequence was therefore impossible — an
early attempt asserted the ROS 2 convention across the matrix and failed the
cpp cell on a difference that was not a defect.

## Fix

1. **#0450 (upstream)** made the Rust server store the accepted `order` and
   compute the sequence, replacing the fixed `[0, 1, 1]`.
2. **C server** — `goal_callback` already parsed `order` for its range check and
   then discarded it (`(void)context`). It now stashes it in the existing
   `server_context_t` (the same `&app.ctx` all three callbacks register with),
   and `accepted_callback` computes from it instead of `int32_t order = 10;`.
3. **C++ server** — moved from `i < goal.order` to `i <= goal.order`, joining
   the ROS 2 `order + 1` convention the other two already followed; the feedback
   trigger moved from `i == goal.order - 1` to `i == goal.order` with it.
4. **Test** — the action rows now assert `FIBONACCI_ORDER_10_SEQUENCE`, which
   had existed in `nros_tests::output` with ZERO users until #0448 wired it into
   `xrce_ros2_interop`.

Bounds checked rather than assumed: both servers reject `order >= 64`, the
generated binding is `int32_t data[64]`, and the loops write indices `0..=order`
— max index 63, exactly fitting.

## Verified

`native_example_reqresp` passes 18/18 cells. Then, because a green assertion
proves nothing until it can fail, the expected sequence was temporarily changed
to `…, 34, 99]` and the run failed **9 of 18** cells — every action cell, all
three languages × all three RMWs — naming each one. Restored; green again, no
residual diff.

## Notes

Filed 2026-08-06 while fixing #0448; resolved 2026-08-07. The remaining sibling
is **#0454**: the `*_send_goal_raw` FFIs never strip the CDR header, so
`PollingActionClient` would reproduce #0448 verbatim — still latent because
nothing instantiates it, which is the same "no consumer, no coverage" shape this
issue was about.
