---
id: 2129
title: "`nros setup` source-build fetches have no retry, so one DNS failure on a hosted runner stops the job"
status: open
type: bug
area: [ci, setup]
severity: low
found: 2026-10-29
related: [phase-466]
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**, since 2026-10-29.

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

## What would close it

Retry the source fetch a few times with backoff, in the one place every
source recipe fetches through (`sdk_store.rs`), and only for a failure that
is not about the ref itself. A missing tag must still fail at once.
