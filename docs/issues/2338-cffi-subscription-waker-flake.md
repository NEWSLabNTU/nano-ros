---
id: 2338
title: "cffi_subscription_waker_fires_on_each_arrival failed once at ~10 s in a merge-queue test-unit run"
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
