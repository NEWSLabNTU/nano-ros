---
id: 2129
title: "`nros setup` source-build fetches have no retry, so one DNS failure on a hosted runner stops the job"
status: resolved
type: bug
area: [ci, setup]
severity: low
found: 2026-10-29
related: [phase-466]
resolved: 2026-10-30
---

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**, since 2026-10-29.

## Measured

`host-tests`' rt-sched job, run 40625461704 (2026-10-28), failed in
**Provision the declared system closure and the Linux scope**:

```
nros setup --tool corrosion: source build 0.6.1-nros1 (no prebuilt for linux-x86_64)
fatal: unable to access 'https://github.com/corrosion-rs/corrosion/': Could not resolve host: github.com
  [FAILED]  corrosion — install 0.6.1-nros1: git fetch --depth 1 v0.6.1 (corrosion): ... failed (exit status: 128)
```

The `E: Unable to locate package qemu-system-arm ...` lines earlier in the
same step are NOT the failure. `nros setup --system` tries `apt-get install`
first and falls back to `apt-get update && apt-get install`, and that
fallback ran and installed everything. A first triage misread them as the
cause.

The job passed on the 7 previous nights, so this is one transient DNS failure
on a GitHub-hosted runner.

## Cause

The SDK store's source recipes run `git fetch` once. A name-resolution blip
during that one fetch fails the whole provisioning step, and with it every
cell behind it.

## Fix

The retry lives in the one command runner, `sdk_store::sh_with_toolchain`,
not at the call sites: eight fetches in `sdk_store.rs` and the rustup-init
download in `rust_toolchain.rs` all go through it, and a new one gets the
retry without asking.

- `is_network_fetch` picks the commands that reach the network: `curl`, and
  `git fetch`, `clone`, `ls-remote` and `submodule update`.
- `run_fetch` runs them up to `FETCH_ATTEMPTS` = 3 times, waiting 5 s and
  then 10 s between attempts.
- It retries only when the output names a network failure
  (`is_transient_network_failure`: `Could not resolve host`, connection
  timeouts and resets, `early EOF`, HTTP 408/429/5xx, and similar). git exits
  128 for a DNS failure and for a missing ref alike, so the exit code cannot
  tell them apart. A missing tag or commit matches none of the markers and
  fails at once, as before.
- The output still streams to the step's log as it happens.
  `step_log::status_keeping_output` also keeps a copy to classify.

Unit tests (`fetch_retry_tests`): a DNS failure runs 3 times and then fails;
a missing ref runs once; a fetch that fails once on DNS and then succeeds
passes on its second attempt; and only network commands count as fetches.

## What would close it

The next `host-tests` rt-sched failure on a DNS blip shows `failed on a
network error (attempt 1/3); retrying` and then passes. Until a blip happens,
the unit tests are the only evidence.

## Resolution

Closed 2026-10-30 by maintainer decision, on the lane's green runs rather than
on an observed retry. The fix (#2805, 39e844300b) retries every network
command the SDK store runs, up to 3 attempts, on a failure whose output names
a network error.

- `host-tests`' rt-sched job passed on 2026-10-29 (run
  [40810937288](https://github.com/NEWSLabNTU/nano-ros/actions/runs/40810937288)) and 2026-10-30 (run
  [40996190687](https://github.com/NEWSLabNTU/nano-ros/actions/runs/40996190687)).
- Neither run hit a network failure, so neither log has a
  "failed on a network error (attempt N/3)" line. The retry path is covered by
  the four `fetch_retry_tests` unit tests only, not by a CI observation.

If a source fetch on CI again stops a job on a DNS or connection error,
reopen this issue with the log: either the error text was not one the retry
recognises, or 3 attempts were not enough.
