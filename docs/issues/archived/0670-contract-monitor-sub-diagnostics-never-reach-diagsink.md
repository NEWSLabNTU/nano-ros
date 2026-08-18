---
id: 670
title: "`contract_monitor_parity` is red on main: the SUB's `/diagnostics` never reach the diagsink under the test harness, while the same three binaries work by hand"
status: resolved
type: bug
area: testing, diagnostics
related: [issue-0671, issue-0445, issue-0471, issue-0480, phase-296, phase-362, rfc-0052]
---

## Symptom

`contract_monitor_violations_report_on_diagnostics` fails on `main`
(174542aba), in a tier-1 `just ci`, and reproduces **solo** — it is not the
load-flake class. It is one of exactly two real failures in that sweep; the
other (`workspace_features_e2e::case_17_mixed_qos`) passed 4/4 solo and IS a
load flake.

The compliant twin in the same file passes, and that is not reassuring: the twin
asserts `/diagnostics` stays SILENT, so a pipeline that reports nothing at all
passes it vacuously. Only the violating case can tell the two apart.

## What the failure actually is

The test's own message could not say, because it was empty (see "Two harness
defects" below). With the waits reporting, the diagsink's full output over 32 s
is:

```
INFO nros_node::executor::spin] nros: session open
INFO contract_monitor_diagsink] cm_diagsink: subscribed; run=32000ms
INFO contract_monitor_diagsink] DIAG rule=rate-hierarchy-runtime hw=/cm/pub/cm_header level=2
```

**One** DIAG line in 32 s. The PUB's rate rule arrives; the SUB's age rule never
does. Meanwhile the sub is demonstrably alive and receiving the stale headers
that should trip it, at 2 Hz for the whole run:

```
INFO contract_monitor_sub] cm_sub: received header stamp.sec=1787023175
... 50 more ...
```

So: `/diagnostics` works cross-process (the pub's rule lands), the sub receives
violating input, and the sub's violations still never reach the observer.

## The same binaries pass by hand

Driven from a shell — same three prebuilt fixtures, same env
(`RUST_LOG=info`, `NROS_LOCATOR`, `NROS_SESSION_MODE=client`, `CM_RUN_MS`,
`CM_PERIOD_MS=500`, `CM_STALE_MS=2000`), same start order and spacing:

| router | max-age DIAG | rate DIAG | sub received |
| --- | --- | --- | --- |
| ROS `rmw_zenohd` (what the test uses) | **50** | 1 | 50 |
| vendored `build/zenohd/zenohd` (retired by phase-362) | 50 | 1 | 50 |

Both routers work by hand. The router is NOT the variable — worth stating
because phase-362 W4 retired the vendored one and that is the obvious suspect.
Repro script: `tmp/cm/repro.sh` (gitignored; regenerate from this issue).

The difference is therefore something about running under the harness, not about
the fixtures, the router, or the contract logic. Candidates NOT yet ruled out:

* `ManagedProcess` pipes the children's stdout/stderr and drains the sub's only
  once (the readiness wait) — but the sub emits ~5 KB over 30 s against a 64 KB
  pipe, so plain backpressure does not obviously explain it;
* the ROS domain / unique-port fixture wiring around `zenohd_unique`;
* start-order timing against zenoh-pico's volatile-by-default matching, though
  the hand runs use the same spacing.

Whoever picks this up: the cheap decisive experiment is to reproduce the
harness's exact spawn (piped, undrained children) from a standalone driver and
bisect toward the shell version, rather than reading more code.

## Two harness defects found on the way — both fixed in this branch

Neither causes the red; both are why it took so long to see it, and both are the
issue-0445 shape ("the verdict replaced the evidence").

1. **`wait_for_output_count` threw away its output on timeout.** It returned
   `TestError::Timeout`, a unit variant, so the one path where the caller most
   needs to see what the process printed was the only path that dropped it. Its
   own sibling branch (process exited) already reported the output, and
   `wait_for_output`/`wait_for_all_output` return `Timeout` only when the output
   is genuinely empty. `param_live_read_e2e` had already worked around this at
   its call site by waiting on a broader pattern, with a comment naming the
   cause — the site was fixed, the class was not.

2. **The call site defaulted the error away.** `unwrap_or_default()` turns the
   error into `""`, which is what made the assertion print `got:` with nothing
   after it.

**Do not fix (2) by folding the error text into the asserted string.** The error
NAMES the pattern it waited for (``did not print `max-age-runtime` ``), so
`seen.contains(RULE_MAX_AGE_RUNTIME)` then matches the *complaint about the
missing rule* and the test passes exactly when it should fail. This was tried
and produced a green run against a pipeline emitting one DIAG line. Evidence
belongs in the panic message; only real output belongs in `seen`.

## The class, and why it was NOT swept mechanically

`unwrap_or_default()` on a wait is not rare: `packages/testing/nros-tests/tests/`
has ~84 `wait_for_output_count` calls and many more `wait_for_output_pattern`
ones, and a good number default the error away. `wait_for_output_pattern`'s
error ALREADY carried the output, so those sites were discarding evidence before
this issue existed.

They are deliberately left alone here. A mechanical `unwrap_or_else(|e|
e.to_string())` sweep would introduce the vacuous-pass hazard above at every site
whose assertion is `contains(<the same pattern it waited for>)` — which is the
common shape. The safe class fix is a helper that returns the real output and the
diagnostic SEPARATELY, so the diagnostic can only reach the panic message:

```rust
fn wait_rule(p: &mut ManagedProcess, pat: &str, secs: u64) -> (String, Option<String>)
```

`contract_monitor_parity` now does this inline. Promoting it to
`nros_tests::process` and converting the call sites is the follow-up; each
conversion needs its assertion checked, not just its `unwrap_or_default`
replaced.

## Why this matters beyond one test

RFC-0052's cross-runtime parity claim rests on this test. With the sub half
silent, the `max_age_ms` subscriber contract is unverified on the Linux runtime
— and the compliant twin cannot notice, because silence is what it asserts.

## Resolved 2026-08-18 — the red was issue 0671; the residual class work is done here

### The failure is gone, and it was never about the harness

This issue's central finding — the PUB's rate rule reaches `/diagnostics` while
the SUB's age rule never does, with the sub demonstrably receiving the stale
headers that should trip it — is exactly the asymmetry
[issue 0671](archived/0671-contract-monitor-reports-nothing-on-diagnostics.md)
diagnosed the day after this was filed:

```rust
if let Some(clock) = config.clock_us {   // GUARDED
    …
}
executor.epoch_us_fn = config.epoch_us;  // NOT guarded  <- the bug
```

`ExecutorConfig::new` — the path `ctx.config()` takes — leaves `epoch_us: None`,
so the unguarded line overwrote the platform default and every hosted node built
that way silently lost its wall clock. Without an epoch,
`Node::subscription` never attaches the age cell, so a baked `max_age_ms`
contract became a dead monitor. The rate monitor rides the GUARDED `clock_us_fn`
and kept firing. One guarded line, one not — the two-sites-one-fixed class.

Verified here on freshly rebuilt fixtures:

```
PASS [ 5.355s] contract_monitor_violations_report_on_diagnostics
PASS [12.586s] contract_monitor_compliant_pair_stays_silent
2 tests run: 2 passed, 0 skipped
```

5.4 s, not the 32 s budget — it now finds the violation instead of waiting the
window out, which is 0671's recorded signature (32 s → 5.2 s).

**So the "cheap decisive experiment" this issue prescribed — reproduce the
harness's piped, undrained spawn from a standalone driver — was aimed at the
wrong layer.** The reasoning that led there was sound given the evidence (the
same three binaries passed by hand), but the variable was not the harness: it
was that the by-hand runs used a config path that still had an epoch. Recorded
because the misdirection was reasonable and still cost time.

### What this issue owned, and what is fixed here

The two harness defects were fixed in `dd177b7fd` (the commit that filed this):
`wait_for_output_count` dropping its output on timeout, and the call site
`unwrap_or_default()`ing the error away. What that commit deliberately left is
the CLASS, and this closes it.

**`ManagedProcess::collect_until_count`** — the `wait_rule` closure
`contract_monitor_parity` carried inline, promoted to `nros_tests::process`. It
returns the real output and the diagnostic on SEPARATE channels, which is the
whole point: both obvious one-liners are traps.

* `.unwrap_or_default()` destroys the evidence — this issue's own empty `got:`.
* `.unwrap_or_else(|e| e.to_string())` folds text that NAMES the awaited pattern
  into the string the test asserts on, so `seen.contains(RULE)` matches the
  complaint about the missing rule and the test passes exactly when it should
  fail. Tried, produced a green run against a pipeline emitting one DIAG line.

`collect_until` is the single-occurrence sibling and already existed; its
doc-comment had named this hazard without a count variant to point at.

**`check-wait-evidence-discarded`** (in `just check`) — because a helper nobody
reaches for is not a class fix. It flags `wait_for_*output*(…).unwrap_or_default()`
across `nros-tests`, with the **87 present sites baselined as a shrinking
backlog** rather than swept: this issue records why a mechanical rewrite is
unsafe, so each conversion needs its ASSERTION read, not just its
`unwrap_or_default` swapped. What the gate buys today is that an eighty-eighth
cannot arrive silently.

Note the population GREW when the class was fixed at source. Before
`dd177b7fd`, `wait_for_output_count`'s timeout was a unit `TestError::Timeout`
with nothing to discard; now every one of those errors carries output, so every
`.unwrap_or_default()` on one is throwing real evidence away. The original
estimate of ~84 was also low by the same kind of margin the fix itself was:
writing the family as `wait_for_output*` misses `wait_for_all_output`, which the
gate's self-test caught before the baseline was taken.

Mutation-checked in both directions: adding one site fails the gate naming the
file and the growth; the seven-case self-test covers both remedies and an
unrelated `unwrap_or_default`.


## CORRECTION 2026-08-18 — "works by hand" was a MUSEUM BINARY, not the harness

This issue's title and its central finding — *the same three binaries work by
hand, so the variable is the test harness* — are **wrong**, and the reason is
worth keeping because it cost hours and produced four confidently-reported
"ruled out by measurement" results that all measured the wrong artifact.

`require_prebuilt_binary` REDIRECTS a leaf-local path onto the shared cargo group
dir (phase-340). So:

* the TEST ran `build/cargo-fixtures/linux/nros-relwithdebinfo/…`;
* the SHELL runs reached a residual
  `packages/testing/nros-tests/bins/contract-monitor/target/…` copy that predated
  the regression.

Two different files behind one path spelling, giving opposite verdicts. Running
the lane's ACTUAL binary from the same shell reproduced the failure immediately,
with no harness involved — which is what led to the real cause (the unguarded
`epoch_us` assignment, fixed under #0671).

Ruled out against the wrong binary, and therefore never actually ruled out:
router type (ROS vs the retired vendored one), undrained pipes, cwd, and
`activate.sh` env. None of them mattered.

**Rule this earns:** when a fixture "passes by hand but fails in the test", the
FIRST check is which file each one ran — `ls -l` on the leaf path and the group
dir — before forming any other hypothesis. The leaf `target/` residue is the
phase-340 P2 shape issue 0488 tracks; it is not merely wasted disk, it is a
second artifact a hand-run reaches and the resolver does not.
