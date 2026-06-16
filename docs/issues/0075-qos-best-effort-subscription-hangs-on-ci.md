---
id: 75
title: best_effort QoS subscription declaration hangs on CI (host-integration) — passes locally
status: open
type: bug
area: rmw
related: [issue-0057, phase-211]
---

`qos_overrides_runtime_delivery::qos_override_best_effort_honored_and_delivers`
(`packages/testing/nros-tests/tests/qos_overrides_runtime_delivery.rs`) is the last
real failure on `host-integration-tests` after #57 (OOM) + #71 (multi-std) cleared
the rest. **CI-only** — it passes locally in ~2 s.

## Symptom

The `listener` fixture process (`qos-override-pubsub`, role=listener,
`NROS_QOS_OVERRIDE=reliability=best_effort`):

1. opens the zenoh session (`Executor::open_with_rmw` succeeds),
2. logs `qos effective: role=Subscription reliability=BestEffort` (the test's first
   wait, 8 s, **passes** — so the session is up and the override is computed),
3. then **emits nothing further** — no `subscription created`, no
   `subscription create rejected` error. The test's second wait
   (`"Waiting for"`) times out with **empty** new output → `subscriber did not
   become ready: Timeout`.

So it hangs **inside `node.create_subscription_raw` → `Session::create_subscriber`
with the best_effort QoS** (`nros-node/src/executor/node.rs:385`,
`nros-rmw-zenoh`), after the session is already open. (Widening the wait 4 s → 12 s
did NOT help — it is a hang, not slowness.)

## Isolation

Same CI run, same nextest parallelism:

- `deployed_native_system_e2e` (native cross-process zenoh e2e, **default/Reliable**
  QoS) — **PASS**. So it is not generic runner contention.
- `qos_default_without_override_is_reliable` (talker-only, no subscription) — **PASS**.
- `qos_override_best_effort_honored_and_delivers` (best_effort **subscription**) —
  **HANG/FAIL**.

→ specific to declaring a **best_effort subscriber** over zenoh-pico in the CI
environment.

## Not reproducible locally

Passes in ~2 s on a dev box (same code path). Needs the CI container / loaded
runner. **Next step:** instrument the fixture or `nros-rmw-zenoh::create_subscriber`
to log around the zenoh-pico `z_declare_subscriber` (or whichever blocking call) and
capture the listener's stderr on the test's timeout, then read it from a CI run —
to see whether the declare blocks, the read/lease task is starved, or the best_effort
reliability flag drives a different (blocking) zenoh-pico path. Compare the
best_effort vs reliable declaration in `nros-rmw-zenoh`'s subscriber create.

## Note

The 4 s → 12 s wait widen (commit `f9d01feba`) was a wrong first guess (assumed
slowness); it is harmless headroom but does NOT fix this — the declaration genuinely
never returns on CI.
