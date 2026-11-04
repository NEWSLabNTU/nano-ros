---
id: 2250
title: "The C++ workspace-entry smoke test started no router, so it passed only while a deleted retry loop kept a failing open alive"
status: resolved
type: bug
area: [testing]
severity: medium
found: 2026-11-04
resolved: 2026-11-04
related: [2207, phase-466, phase-521]
---

## Measured

run-matrix 41784686434 (schedule, 2026-11-04, `main` at 8c49a148), tier 1:
one real failure out of 3440, which was new:

```
nros-tests::workspace_metadata cmake_cpp_workspace_entry_starts_prebuilt_runtime
workspace_metadata.rs:180:5: C++ workspace Entry should enter its native spin loop when started from the prebuilt binary
```

The last tier-1 run that reached its cells and passed was on 2026-10-31 at
f5c7bb5d.

Rebuilt the `workspace-cpp-native` fixture on this host and ran
`linux_entry` the way the test spawns it, with no environment:

```
[ERROR] nros: zpico Session -> ConnectionFailed (the two are indistinguishable downstream)
[ERROR] nros: RMW session open failed — ConnectionFailed
```

It exited 156 within the test's 300 ms. Against a private `rmw_zenohd`
(`NROS_LOCATOR=tcp/127.0.0.1:17447`, client mode), the same binary opened its
session and published `Published: 0, 1, …` and received the same values,
still running when it was stopped at 3 s.

## Cause

The runtime was fine; the test was wrong. It started no router and asserted
only `proc.is_running()` 300 ms after spawn. That held because
`Context::connect_with_retry` (ten attempts, 300 ms apart) kept a doomed open
alive for about 3 s. 55a467ea51 (issue 2207) deleted that loop, so an open
with no router now fails at once, and the assertion went red. 2207's sweep fixed
the one test that relied on the loop on purpose
(`test_rtos_session_open_outlasts_a_late_router`); this one relied on it by
accident, and "alive at T" could not tell the two apart.

Sibling sweep: every `is_running()` assertion under
`packages/testing/nros-tests/tests` (`git grep -n -B12 'is_running()'`). The
others all start a router or an agent, and none failed in the same run.

## Fix

The test now starts `ZenohRouter::start_unique()`, passes `NROS_LOCATOR` and
`NROS_SESSION_MODE=client` (as `rust_workspace_entry_runs_prebuilt_pubsub_e2e`
already did), and waits up to 10 s for the entry to publish
(`INT32_TALKER_LOG_PREFIX`). It then asserts that the output reports the
session open (`SESSION_OPEN_MARKER`) and that the process is still running.
Each failure message carries the captured output. Solo run: PASS in 1.35 s.
