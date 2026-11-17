---
id: 2665
title: "`target_session::tests::teardown_waits_for_a_chain_that_is_stopping` fails under host load: its chain must start within 5 s"
status: open
type: bug
area: [testing, cli]
severity: low
found: 2026-11-17
related: [phase-478, issue-2307, issue-2603]
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md),
next to issue 2603, the other CLI unit test that fails under load and passes
alone. The test belongs to issue 2307's fix (phase-544, archived).

## What happens

`cargo test -p nros-cli-core` on a host at load average about 390 (80 cores,
mostly other users' jobs) failed one test of 2,529:

```
target_session::tests::teardown_waits_for_a_chain_that_is_stopping --- FAILED
panicked at nros-cli-core/src/target_session.rs:876:13
```

Line 876 is in the helper `runner_with_chain`. It is the
`"the chain never started"` assertion: after the runner prints its chain line,
the helper polls `group_members(pgid)` until the `setsid` child shows up, under
one 5 s deadline that also covers waiting for the chain line. Run alone
straight after, the test passed 3 of 3 in about 0.5 s.

So under load the runner shell, its `setsid` and the chain's first process did
not all get scheduled within 5 s. The teardown behaviour the test checks was
never reached.

## Why it matters

The failure says nothing about the teardown. It reads like a regression in
issue 2307's fix, and in `just ci gate` it would fail `check-cli-tests` for a
reason that is only about the host.

## Possible shape

- The helper's deadline bounds the SETUP, not the behaviour under test. So it
  can be much longer (say 60 s) without weakening the test: a chain that
  never starts still fails, only later.
- The test's own assertion (`t.elapsed() < CHAIN_GRACE`) measures the
  teardown and is a separate question. Check that one under load as well
  before changing anything.

Not fixed in the change that filed this, by request.
