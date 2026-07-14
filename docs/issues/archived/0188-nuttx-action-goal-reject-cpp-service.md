---
id: 188
title: "nuttx C/C++ action e2e: goal REJECTED at handshake (ret=-2); nuttx cpp service also red"
status: resolved
type: bug
area: nuttx
related: [issue-0179, issue-0153, phase-287]
resolved_in: "2026-07-14 gossip-gap retry in the three nuttx clients (#153 fix shape)"
---

## Summary (as filed)

With #179 landed, the nuttx lanes stayed red with a different signature:
C/C++ action "goal rejected (ret=-2)" and C++ service red, while nuttx
pubsub + C service passed on the same images.

## Root cause — the #153 gossip gap, unported to the nuttx C/C++ clients

Fresh-fixture triage (staleness excluded by the #182/#185 guards):

- `ret=-2` is `NROS_RET_TIMEOUT`, not a rejection: the send-goal / service
  query is sent, and the blocking wait expires. The action server never logs
  a goal request — the query never reaches it.
- The C++ service client runs its whole flow and its future wait fails with
  the same -2 after its 5 s budget (an earlier lossy grep misread this as a
  post-banner hang).
- Mechanism: on zenoh the server's readiness gossips ahead of its queryable
  route; a query fired in that window matches no queryable — and a zenoh get
  is evaluated against the queryables visible at fire time, so a longer wait
  on the SAME query can never recover. Exactly issue 0153, which was fixed
  in the **native rust** demos (3-attempt / 1 s-backoff retry, comment
  naming this window) but never ported to the nuttx C/C++ clients.
- Why the siblings passed: rust nuttx clients inherited the #153 retry; the
  C service client's single call rides a 30 s in-call budget
  (`NROS_SERVICE_TIMEOUT_MS`) and fires late enough after its own QEMU cold
  boot; the failing three were one-shot with 5–15 s budgets.

## Fix

Ported the #153 retry (3 attempts, ~1 s executor-spin backoff between
fresh queries — spinning, not sleeping, so keep-alives/gossip keep being
serviced) to:

- `examples/qemu-arm-nuttx/c/action-client/src/main.c` (send_goal)
- `examples/qemu-arm-nuttx/cpp/action-client/src/main.cpp` (send_goal)
- `examples/qemu-arm-nuttx/cpp/service-client/src/main.cpp`
  (fresh `send_request` per attempt)

Retries only on `-2`/timeout; real rejections still fail immediately.

## Verified

All six nuttx action+service lanes serialized: 6/6 PASS (the three fixed
lanes plus the previously-green rust/C siblings), then the three fixed lanes
2× more serialized — 3/3, 3/3. Fixtures freshly rebuilt before every run.

Class note: the freertos/threadx C/C++ clients are separate copies without
the retry — green today via faster boots, but the same latent window exists;
worth folding into any future example-dedup pass.
