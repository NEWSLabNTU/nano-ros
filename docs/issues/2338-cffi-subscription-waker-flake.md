---
id: 2338
title: "cffi_smoke sessions sometimes fail to open against their own router in merge-queue test-unit (seen as the waker test, ~10 s)"
status: open
type: bug
area: [rmw, zenoh, testing, ci]
severity: low
found: 2026-11-07
related: [1940, 1976, 2457]
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)** — a
merge-gating lane that ejects on a test whose failure the PRs in the batch could
not have caused is a verdict without signal.

## Measured

Merge-queue `gate` run 42330498974 (2026-11-07 02:41 UTC), job 128275682326,
step `check / test-unit (merge group + nightly/manual)`, ejected the batch
holding #3235 (issue 2294, CLI `nros check`) and #3240 (a phase doc):

    FAIL [  10.187s] (2410/2411) nros-rmw-zenoh::cffi_smoke cffi_subscription_waker_fires_on_each_arrival
    Summary [  42.829s] 2411 tests run: 2410 passed, 1 failed, 6 skipped

The neighbouring `zenoh_integration client_open_against_a_silent_listener_fails_within_a_bound`
took 10.165 s in the same run, so the failing test ran to a ~10 s bound rather
than failing fast. The log carries no panic text for it; the stdout block
printed beside it is the issue-1976 churn test's
(`zpico: z_declare_publisher failed: -100`), not this test's.

## What it is NOT

- Not either PR: #3235 merged a few minutes later through a fresh batch with
  the same test green, and #3240 changes three Markdown files.
- Not issue 1940 (resolved): that was the wake callback being inert through
  cffi, i.e. a test that never passes.

## What would close it

Either a second sighting with the panic text (the assertion and the elapsed
arrival count), which tells whether arrivals were missed or merely late under a
loaded runner, or a change that makes the test's bound a function of what it
measures rather than a fixed ~10 s window. If it does not recur within two
weeks of merge-queue traffic, close as not reproduced.

## No second sighting, 2026-11-12

The failed merge-queue `gate` runs from 2026-11-07 03:00 UTC to 2026-11-12 (61
runs) were searched for `cffi_subscription_waker`. It failed in none of them.
Two of them name `cffi_smoke`, and in both the waker test passed: 42686592265
(1.633 s) and 43130480680 (1.654 s).

Locally, 96 runs of the test at 32-way, each with its own `zenohd`, all passed
in 1.62–1.64 s. That time is the test's 1 s sleep plus three arrivals.

43130480680 did fail a sibling, `cffi_pubsub_round_trip`:
`CffiSession::open` returned `ConnectionFailed` at 10.168 s
(`cffi_smoke.rs:100`). That is a failure to connect to the test's own router.
The 10 s matches the connect budget #3566 introduced (issues 2389/2437/2438,
merged 2026-11-11 22:16 UTC), so it belongs with that change, not with this
issue's late or missed wakes.

This issue's closing rule stands: close as not reproduced if nothing recurs by
2026-11-21, two weeks after it was found.

## Second sighting, 2026-11-15: the session never opened

Merge-queue `gate` run 43701411501 (2026-11-15 10:01 UTC), job 132773193591,
step `check / test-unit (merge group + nightly/manual)`, ejected #3955 (a test
of the Renode runner and two docs, nothing under `packages/rmw`):

    FAIL [  10.189s] (2524/2525) nros-rmw-zenoh::cffi_smoke cffi_subscription_waker_fires_on_each_arrival
    Summary [  12.470s] 2525 tests run: 2524 passed, 1 failed, 7 skipped

This time the log carries the panic, and it answers the question above: no
wake was missed or late. The test never reached a subscription.

    panicked at packages/rmw/zenoh/nros-rmw-zenoh/tests/cffi_smoke.rs:95:19
    zenohd: accepting on tcp/127.0.0.1:37655 100 ms after spawn
    [nros_log ERROR] nros: zenoh open: no session with tcp/127.0.0.1:37655 in one attempt (connect_timeout_ms=0) (refused, unreachable, or no answer to the handshake)
    [nros_log ERROR] nros: zpico Session -> ConnectionFailed (the two are indistinguishable downstream)

Line 95 is `open_client`'s `Err` arm: `CffiSession::open` returned
`ConnectionFailed` against the test's own router, which had reported accepting
100 ms after spawn. That is the same shape as `cffi_pubsub_round_trip` in
43130480680 (above): one open attempt, ending at zenoh-pico's ~10 s handshake
bound. So both sightings, and the sibling, are a client open against a
listening local router that gets no handshake answer within 10 s under a loaded
runner. They are not this test's waker. In the same run,
`declares_keep_running_across_a_reconnect` and its router churn finished in
the seconds just before.

What would close it changes accordingly: the open path, not the waker. Either
measure why one handshake against a router already accepting goes unanswered
for 10 s on the runner, or give these smoke tests' open a retry within a stated
budget, the way the reconnect tests already ride out an outage.
