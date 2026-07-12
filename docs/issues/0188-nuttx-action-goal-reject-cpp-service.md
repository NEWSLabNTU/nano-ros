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

## Notes

- Whether the server ever SEES the goal (server boot output) not yet
  captured — first triage step: grep the server side for
  "Received goal request".
- These lanes were never harness-green before phase-287 (old images baked
  port 7447 — see #179's history), so no known-good baseline exists.
- cpp service red on nuttx only — possibly the same underlying
  request/response quirk on the nuttx net stack (send_goal IS a service
  call); triage together.
