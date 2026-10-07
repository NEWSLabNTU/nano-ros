---
id: 1756
title: "The native async action client gets its goal accepted and then never resolves the awaited result"
status: open
type: bug
area: [rmw, testing]
severity: medium
found: 2026-10-08
related: [1013, 1026]
---

## Measured

`run-matrix` run **37685900447** on `main` (workflow_dispatch, 2026-10-07 20:57Z),
job **113036322535** `tier 1 (cells)`. This was the first tier-1 run on the
self-hosted runner. The test failed after 40.3 s:

```
nros-tests::native_async_roundtrip_e2e native_async_action_client_awaits_goal_and_result
panicked at packages/testing/nros-tests/tests/native_async_roundtrip_e2e.rs:126:5:
async action client accepted the goal but never resolved its awaited RESULT — the .await path
stalled after acceptance (feedback stream or get_result), which the old goal-acceptance-only
assertion reported as a pass.
[INFO] action-client-async: Sending goal
[INFO] action-client-async: Goal accepted by
```

The client log also shows that every action client endpoint was granted less
history depth than it asked for:

```
[WARN] nros: qos: client '/fibonacci/_action/send_goal' asked for KEEP_LAST(10); this image's receive ring holds 4. Granting 4 …
[WARN] nros: client request publisher `/fibonacci/_action/get_result`: asked for history depth 10 and the backend granted 4.
```

This is one of three "real failures" in that run's junit rewrite. The other
two are the `~/.cache` permission error (issue 1755) and a PX4 fixture that the
lane never built (issue 1685).

## What it is NOT

- **Not a skip.** The junit rewrite counts it as a real failure, and it ran for
  40 s against a live router.
- **Not a known flake.** No open issue names this test. Its assertion is the
  strengthened one: it now requires the awaited result, where it used to accept
  goal acceptance alone.

## Not yet known

It is not known whether this is the `.await` path or the reduced depth. The
first thing to establish is whether it reproduces solo on a host. If it does,
re-run with `ZPICO_SUBSCRIBER_RING_DEPTH` raised to 10. If that makes it pass,
the depth grant is losing the `get_result` reply. If it still fails, the async
`get_result` / feedback path stalls.

## What would close it

The test passes on two consecutive self-hosted tier-1 runs, and the cause above
is named.
