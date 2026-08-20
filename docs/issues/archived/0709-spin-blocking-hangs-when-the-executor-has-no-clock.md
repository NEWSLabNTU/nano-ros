---
id: 709
title: "`spin_blocking(timeout)` hangs FOREVER when the executor has no clock —
  and `from_session` has no seam to give it one"
status: resolved
type: bug
area: core
related: [phase-359, issue-0687, issue-0669]
---

## Symptom

With `nros-node` built `std` and NOT `rmw-cffi` — the mock-session
configuration — an executor whose `clock_us` is `None` never leaves
`spin_blocking`:

```
test executor::tests::test_spin_blocking_timeout ... <hangs>
```

The test asks for `timeout_ms(50)` and asserts it returns within 2 s. It does
not return at all. Killed after 10 hours on the first encounter, reproduced in
300 s.

## How it was found, and what it invalidates

phase-359 W10 tried to delete the `std`-without-a-port clock fallbacks
(`default_std_clock_us`, `default_epoch_us`'s `SystemTime` arm). The evidence
for "they are dead" was that deleting them builds `nros-rmw-metadata`,
`nros-tests` and the whole workspace `--all-targets` with **zero errors**.

That evidence was insufficient and the conclusion was wrong. **Compiling is not
running.** No caller REFERENCES the fallback — it is a default, selected by
`default_clock_us_fn()` — so removing it cannot produce a compile error
anywhere. It produces a hang, and only a test that actually spins can see it.

The deletion was reverted (257 `nros-node` tests, 1.45 s).

## Two defects, and only one of them is the campaign's

**1. The seam that was supposed to make the fallback unnecessary does not exist
on this path.** The argument for deleting was "a caller that has a clock but no
port supplies it — `ExecutorConfig::clock_us` is the seam". But the no-port
population reaches the executor through

```rust
pub fn from_session(session: session::ConcreteSession) -> Self
```

which takes **no config at all**. Issue 0687 named exactly this population
(`Executor::from_session` accepts any `Session`; a non-cffi backend is a
supported consumer), and it has no way to install a clock. So the fallback is
load-bearing until `from_session` gains a config-taking sibling, or the
population is declared unsupported.

**2. A no-clock executor should FAIL, not spin.** This is the real bug and it
is independent of the campaign. `spin_blocking(SpinOptions::timeout_ms(50))`
cannot observe time passing without a clock, so it loops forever — a hang, in
the one API whose entire contract is "returns after N ms". The repo's rule is
fail-loud: an unmet precondition is an error, never a silent wrong behaviour,
and a hang is worse than either. The fallback has been masking this since the
executor gained one.

## What to decide

1. **Make it fail loud.** `spin_blocking`/`spin_period` with a timeout and no
   clock is a configuration error — return `NodeError` (or panic at
   construction, where the information is). Cheapest, and it is the repo rule.
2. **Give `from_session` a config.** `from_session_with(session, &config)`,
   so the no-port consumer can install `clock_us` the way a board does. This is
   what would let W10 delete the fallbacks.
3. Both. (1) is a bug fix; (2) is the API change W10 needs. They are
   independent.

Until one of them lands, the `std`-without-a-port fallbacks stay, and the
census keeps `nros-node` at 6 `std::` paths rather than 0.

## Resolved 2026-08-20

**The hang is an error now.** `spin_blocking` with a timeout and no clock
returns `NodeError::NotInitialized` and logs which knob to set, instead of
looping until halted. `spin_period` gained the same guard for the same reason —
a period it cannot pace is a busy-loop pretending to run at `period`. An
UNTIMED `spin_blocking` still runs until halt, because that is a promise a
clockless build can keep.

**`Executor::from_session_with(session, &config)`** is the seam `from_session`
never had: a caller that brings its own session can now bring its own clock.
Only the timing sources are read from the config — identity belongs to the
session the caller already opened — and a `None` field does not clobber the
platform default, which is the rule issue 0671 records for `open_in`.

### Three test versions hung before one didn't, and that is the finding

The first observed the clock through `spin_blocking`, with a stub returning a
CONSTANT: no deadline is ever reached, so the test hung on the same defect it
was written for. The second advanced the stub on every READ — which breaks any
loop that re-reads the clock, because the deadline moves with the reader. The
third used `spin_one_period_timed`, which also spins.

The version that works observes a BOUNDED call (`spin_once`) and the installed
field. **A clock is not a free variable in a test**: code that reads it twice
per iteration constrains what a stub may do, and a stub that ignores the
constraint produces a hang rather than a failure.

(Two of those "hangs" were also a ghost: a pre-revert test binary from the
original ten-hour run was still alive and holding the target-dir lock, so later
`cargo test` invocations appeared to hang when they were queued behind it.
`pgrep -f nros_node-` before believing a hang.)

### What is NOT fixed

`spin_one_period_timed` still reports `elapsed: 0, overrun: false` when there is
no clock — a measurement stated as fact that was never taken. It returns
`SpinPeriodResult`, which has no error channel, so making it honest is an API
shape decision rather than a guard. Left deliberately, recorded here.

And the W10 question this issue was found by is still open: the
`std`-without-a-port clock fallbacks stay until someone rules on whether the
no-port population is supported. `from_session_with` removes the mechanical
objection — that population now HAS a seam — but "they can install a clock" is
not the same as "they must".

## Reproduce

```
cargo test -p nros-node --lib --features std -- --test-threads=1
# passes today; to see the hang, first delete the `std` arm of
# `default_clock_us_fn` in packages/core/nros-node/src/executor/spin.rs
# so the no-`rmw-cffi` build resolves to `None`.
```
