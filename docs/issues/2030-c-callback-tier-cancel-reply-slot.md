---
id: 2030
title: "The C callback tier never takes a cancel reply, so the fifth rclc_action_cancel_goal is refused"
status: open
type: bug
area: [api]
severity: medium
found: 2026-10-24
related: [2027, 2009, 0796, phase-515]
---

**Home: [phase-515](../roadmap/phase-515-api-stock-shape-review.md)** — homed 2026-10-24; listed in that phase's table of open API issues.

## What

Found by inspection while fixing issue 2027 (the C++ goal-slot leak); not yet
measured.

Since issue 2009 each action sub-client keeps its requests in a bounded table
(`ActionRequests`, `MAX_CONCURRENT_GOALS` = 4), and a slot comes back only when
its reply is TAKEN. On the C callback tier nothing takes a cancel reply:
`rclc_action_cancel_goal` (`packages/api/nros-c/src/action/client.rs`) sends the
request, and neither the executor's arena dispatch
(`action_client_raw_try_process`: acceptance, feedback, result) nor
`rclc_action_client_poll` reads the cancel sub-client, and the callback tier
has no `try_recv_cancel_response` of its own (only the L1 polling client
does). So after four cancels the table is full even with a healthy server
answering every one, and the fifth is refused (`RequestInFlight`).

The C++ callback tier has the same shape but a reader:
`Client<A>::try_recv_cancel_response`, or since issue 2027 the
`async_cancel_*` futures, take the reply. A C++ caller that cancels and never
reads the reply hits the same wall.

## Fix direction

Either a cancel-response callback on the C client, dispatched by the arena
(which then needs a fourth callback slot in `ActionClientRawArenaEntry`, sized
in `nros-node/build.rs`), or drain-and-drop of cancel replies in the C path
when no reader exists. A C runtime test on the stub backend: five
`rclc_action_cancel_goal` with staged replies, all sent.
