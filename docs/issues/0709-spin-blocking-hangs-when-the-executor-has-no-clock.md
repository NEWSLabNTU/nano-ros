---
id: 709
title: "`spin_blocking(timeout)` hangs FOREVER when the executor has no clock —
  and `from_session` has no seam to give it one"
status: open
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

## Reproduce

```
cargo test -p nros-node --lib --features std -- --test-threads=1
# passes today; to see the hang, first delete the `std` arm of
# `default_clock_us_fn` in packages/core/nros-node/src/executor/spin.rs
# so the no-`rmw-cffi` build resolves to `None`.
```
