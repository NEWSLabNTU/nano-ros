---
id: 670
title: "`contract_monitor_parity` is red on main: the SUB's `/diagnostics` never reach the diagsink under the test harness, while the same three binaries work by hand"
status: open
type: bug
area: testing, diagnostics
related: [issue-0445, issue-0471, issue-0480, phase-296, phase-362, rfc-0052]
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
