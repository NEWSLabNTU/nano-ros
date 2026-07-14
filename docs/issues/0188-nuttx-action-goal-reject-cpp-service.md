---
id: 188
title: "nuttx C/C++ action e2e: goal REJECTED at handshake (ret=-2); nuttx cpp service also red"
status: open
type: bug
area: nuttx
related: [issue-0179, phase-287]
---

## Summary

With the #179 reply-framing fix landed (freertos + threadx-linux action e2e
4/4 green, native matrix 5/5), the nuttx lanes still fail — but EARLIER in
the flow and with a different signature:

```
rtos_e2e test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_{2_C,3_Cpp}:
  Client: Sending goal
  Client: Goal was rejected by server (order=10, ret=-2)
rtos_e2e test_rtos_service_e2e::platform_2_Platform__Nuttx::lang_3_Lang__Cpp — red
```

`ret=-2` on `nros_action_send_goal` (order=10 is valid; the server accepts it
on every other platform) — the send-goal request/response handshake fails on
nuttx, before any feedback/result exchange. Serialized run (`-j 1`), fresh
fixtures (2026-07-13). nuttx C/C++ PUBSUB + C SERVICE pass on the same
images, so transport + pub/sub + basic request/reply work.

## Triage 2026-07-14 (static — corrects the framing)

**`ret=-2` is `NROS_RET_TIMEOUT`, NOT a rejection.** `NROS_RET_REJECTED` is `-13`
(`nros_generated.h`). The client example mislabels it: `send_goal` returning ANY
`!ret.ok()` prints "Goal was rejected by server":

```cpp
// examples/*/cpp/action-client/src/main.cpp:59-64
ret = client.send_goal(goal, goal_id);
if (!ret.ok()) {
    fprintf(stderr, "Goal was rejected by server (order=%d, ret=%d)\n", order, ret.raw());
    ...
}
```

So the send-goal request→response round-trip **times out** on nuttx — the server
most likely ACCEPTED the goal; the accept-response just doesn't reach the client's
spin within budget. **This flips the investigation** from "why does the server
reject?" to "why doesn't the goal-response come back in time on nuttx?".

- Mechanism: `nros_action_send_goal` (`nros-c/src/action/client.rs:449`) =
  async-send + spin-the-executor-until-accepted/rejected/timeout. On nuttx the spin
  hits `NROS_RET_TIMEOUT`.
- **#179 does NOT cover this.** #179 (`09cdeca9a`) fixed the action RESULT
  deserialize (unified zenoh encap contract) — a LATER stage. #188 fails at the
  send-goal HANDSHAKE, upstream of #179's fix. Separate.
- Two failing signatures, likely one area:
  - `send_goal` (C AND cpp) times out ⇒ not language-specific ⇒ specific to the
    **SendGoal service** (UUID + goal) vs the plain AddTwoInts service that passes.
  - cpp AddTwoInts service red while C passes ⇒ a C++-service-on-nuttx issue.
  Both are request/reply round-trips timing out on nuttx's net stack.

**Next (needs the qemu e2e, not yet run):** capture the server side — does it print
"Received goal request" (server SEES the goal)? Does it send the accept-response?
Does the client's spin receive it? This is the layer-boundary evidence that pins
which side drops the reply.

## Notes

- Whether the server ever SEES the goal (server boot output) not yet
  captured — first triage step: grep the server side for
  "Received goal request".
- These lanes were never harness-green before phase-287 (old images baked
  port 7447 — see #179's history), so no known-good baseline exists.
- cpp service red on nuttx only — possibly the same underlying
  request/response quirk on the nuttx net stack (send_goal IS a service
  call); triage together.

## Triage 2026-07-14 (fresh fixtures, serialized 3× each)

Reproduced on freshly rebuilt nuttx fixtures (post-#182/#185 guards, so
staleness is ruled out this time). Rust action + C service PASS on the same
images; C/C++ action + C++ service fail deterministically. New facts:

- **The server never sees the goal.** Server output stops at "Waiting for
  action goals"; no goal-request line ever appears (the filed open question,
  answered). The failure is on the REQUEST leg, not accept/reject logic.
- **`ret=-2` is `NROS_RET_TIMEOUT`**, not a rejection: `nros_action_send_goal`
  = `send_goal_async` (returns OK) + a blocking executor-spin wait for the
  goal response, which expires. The current client build prints
  `Failed to send goal: -2` (the issue's "Goal was rejected" wording came
  from an older client print site — same underlying -2).
- **The C++ SERVICE client stalls even earlier**: it prints its banner
  (`nros C++ Service Client (AddTwoInts)`) and then NOTHING — no call
  attempt, 0 responses, for the full 60 s window, 3/3 tries. Distinct stall
  shape from the C action client (which does send + wait). So there may be
  TWO defects: (a) send_goal query never matching the server's queryable on
  nuttx (gossip-gap class — the #153 "query before queryable visible"
  family, plausible on QEMU's slow guest boot), and (b) a C++-client init
  hang after banner (before any request I/O).
- Suggested next steps: (a) capture the zenohd router log during the C
  action lane to see whether the send_goal query has a matching queryable
  when it fires, and add/verify a gossip-gap backoff on the nuttx action
  client like the demo-client one (#153); (b) attach to the cpp service
  client under QEMU (or add a post-banner probe print) to find the init
  stall point — compare against the PASSING nuttx C service client's init
  order.
