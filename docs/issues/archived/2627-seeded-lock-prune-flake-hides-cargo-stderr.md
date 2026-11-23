---
id: 2627
title: "A seeded leaf lock's prune failed once under a full gate, and the failure shows no cargo error"
status: resolved
type: bug
area: [cli, testing, cargo]
severity: low
found: 2026-11-16
resolved: 2026-11-23
related: [2603, 2683]
---

**Home:** no phase. Found while gating issue 2623's fix,
which does not touch this code.

## Symptom

`check::build` failed in a full solo `just ci gate` on
`fix/freertos-cmake-derived-zenoh-heap` (2026-11-16), on one test:

```
dev_bindings_runs_c1_and_configures_no_image --- FAILED
panicked at nros-cli-core/tests/configure_verb.rs:221:40:
`nros dev bindings` runs: "sync: `cargo update --workspace` failed (exit status: 101)
pruning the lock seeded at …/tmp/configure-bindings-…/listener/Cargo.lock;
the seeded lock was removed, so the next run seeds it again"
```

The other 21 tests in `configure_verb` passed in the same run, and many of
them ran for over 60 s side by side. Run alone right afterwards, the same test
passed in 16 s.

## What is missing

The message names cargo's exit status and nothing else. `prune_seeded_lock`
(`cmd/ws.rs`) runs `cargo update --workspace` with its stderr going to the
parent process, and the test harness keeps only the error chain. So the
failing run's cargo error is gone, and nobody can tell whether it was a
registry fetch, a lock on a shared cargo directory, or something in the
seeded lock.

## Next step

1. Capture cargo's stderr in `prune_seeded_lock` and put its last lines in the
   error. Then the next occurrence explains itself.
2. With that in place, check whether parallel `configure_verb` tests contend
   for a shared `CARGO_HOME` or registry lock. That is the likeliest cause
   when the test fails only under load.

## Status (2026-11-18): step 1 done, the cause still unmeasured

`prune_seeded_lock` now captures cargo's stderr. A failed prune's error ends
with cargo's last 20 non-blank lines ("cargo said: …"), and a successful
prune passes the captured stderr through, so the user sees what cargo printed
before. `failed_prune_removes_the_seeded_lock` now asserts that cargo's own
message reaches the error.

Not done: step 2. The failure has not recurred since 2026-11-16, so whether
parallel `configure_verb` tests contend for a shared `CARGO_HOME` or registry
lock is still unknown. The next occurrence names cargo's reason; close this
issue from that. It is not closed now, because a diagnostic alone fixes no
cause.

A related finding, measured under issue 2603: `nros-cli-core`'s lib tests
share one process, and a test that writes a digest-input variable makes other
tests' stamps read "inputs changed". `configure_verb` is an integration test
that runs the `nros` binary as a child process, so that cause cannot reach
it.

## Resolved (2026-11-23): the failing test no longer prunes online

Issue 2683 (2026-11-20) made `configure_verb` resolve online once per process
and pass `--offline` to every later verb. That fix chose verbs by their first
word (`configure`, `sync`, `build`) and so missed `nros dev bindings`, which
flattens the same sync arguments. The one test that drives it,
`dev_bindings_runs_c1_and_configures_no_image`, is the test that failed here.
So it was still the only test in the file whose prune ran `cargo update
--workspace` online, beside 21 siblings that all ran offline.

Fix: the test's `offline` helper now asks the CLI's parser whether a verb
takes `--offline`, instead of listing verbs. `dev bindings` gets the flag, and
so will any verb that gains it later.
`offline_is_added_wherever_the_grammar_takes_it` checks this. Mutation-checked:
when `dev` is excluded from the flag, that test fails, naming `dev bindings`.

What this does and does not show:
- The 2026-11-16 cargo error was never captured, so it is still not known
  whether the failure was the registry fetch, a lock on the shared cargo home,
  or the seeded lock itself. Removing the online step removes the likeliest
  of the three, the one a loaded host stretches, from this test. It is not a
  measured cause.
- The step-1 diagnostic stays. An offline prune that fails, in this test or
  for a user, ends its error with cargo's own last lines.
