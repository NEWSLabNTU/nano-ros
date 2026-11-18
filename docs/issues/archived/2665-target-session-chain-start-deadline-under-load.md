---
id: 2665
title: "`target_session::tests::teardown_waits_for_a_chain_that_is_stopping` fails under host load: its chain must start within 5 s"
status: resolved
resolved_in: "the change that archives this file"
type: bug
area: [testing, cli]
severity: low
found: 2026-11-17
related: [phase-478, issue-2307, issue-2603]
---

**Home:** [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md),
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

## Resolution (2026-11-18)

**Cause: a race, not only a short deadline.** The teardown test's chain was
`sh -c 'sleep 0.5; touch <mark>'`, which ends on its own half a second after
it starts. `runner_with_chain` then polls `group_members(pgid)` for it. When
a loaded host delays that first look by more than half a second, the chain
has already ended, the group is empty for good, and the helper reads "the
chain never started" until its deadline. A longer deadline alone would only
have made that failure slower.

**Proof, deterministic** (`runner_with_chain_seen_after` looks a set time
late, which is what load did by accident):

- `a_short_chain_is_gone_before_a_late_look`: a `sleep 0.5` chain looked at
  1 s late panics with "the chain never started", every run
  (`#[should_panic]`). This is the reported failure, reproduced without load.
- `a_gated_chain_is_seen_by_a_late_look`: the gated chain the teardown test
  now uses is found 1 s late, and the teardown still ends it.

Load alone did not reproduce it on newslab-118 (32 runs at load ~90, and 40
runs with the test and 40 busy loops pinned to one core), so the late-look
tests are the evidence, not a run count.

**Fix:**

- The teardown test's chain waits for a `go` file before its half second
  starts, and the test writes `go` just before the teardown. So the chain is
  alive whenever the helper looks, and its half second still overlaps the
  teardown as the test means.
- The helper's deadline is 60 s, because it bounds the setup (a shell,
  `setsid`, the chain's first process), not the behaviour under test. A chain
  that never starts still fails.

**Not changed:** the behaviour assertion `t.elapsed() < CHAIN_GRACE` (2.5 s
against a 0.5 s chain). It measures the teardown and did not fail in any run
here.

Sweep: `git grep -nE 'sleep 0\.[0-9]+' -- packages/cli/nros-cli-core/src/target_session.rs`
(the one other short chain is `teardown_kills_a_chain_that_does_not_stop`'s
`sleep 30`, which outlives any look).
