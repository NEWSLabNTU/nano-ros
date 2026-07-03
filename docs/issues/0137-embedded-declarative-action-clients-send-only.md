---
id: 137
title: "Embedded declarative action clients are send-only — no feedback/result seam, client result line unobservable"
status: open
type: enhancement
area: codegen
related: [phase-277]
---

## Summary

The declarative (Node-pkg / `_entry`) action **clients** on the embedded
platforms — `examples/qemu-arm-freertos/rust/action-client`,
`examples/qemu-arm-nuttx/rust/action-client`, and the baremetal RTIC variant
`examples/qemu-arm-baremetal/rust/action-client-rtic` — can only *send* a
goal (`TickCtx::send_goal_for_name` on first tick). The declarative dispatch
layer does not yet wire the action-client response channels
(`GoalStatusArray` topic, feedback stream, get_result future) through to
`ExecutableNode::on_callback`, so the demo-parity transitions from
`action_tutorials` ("Goal accepted by server, waiting for result" /
"Next number in sequence received: [...]" / "Result received: [...]") cannot
be logged. The client's terminal `Result received:` line is therefore
**unobservable** on these platforms.

Action **servers** are fine — the server side round-trips end-to-end (native
xprocess E2Es assert the client's `Result received:` line against native
clients, which use the imperative `send_goal()`/`get_result()` Promise API).

## Current state (phase-277 W5)

- The example sources carry the seam explicitly: `on_callback` bodies are
  empty with a comment marking where feedback/result callbacks land once
  codegen wires the subscriber channels (see
  `examples/qemu-arm-freertos/rust/action-client/src/lib.rs:49-53`).
- The platform E2E tests were retargeted to what *is* observable
  (boot + connect + `Sending goal`), and — per the tests-must-fail rule —
  assert hard on those preconditions rather than soft-passing on a missing
  result line.

## Fix direction

Extend the declarative codegen/dispatch wiring (the same mechanism that
routes topic subscriptions into `on_callback`) to register the action
client's status/feedback/result channels, then:

1. Fill the `on_callback` seam in the three examples with the
   `action_tutorials` wording.
2. Re-point the freertos/nuttx/baremetal-RTIC action-client E2Es at the
   terminal `Result received: [...]` marker.
