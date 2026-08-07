---
id: 471
title: "`wait_for_output_pattern` returns Ok on timeout whenever the process printed anything, so ~233 call sites assert nothing"
status: open
type: bug
severity: high
area: testing
related: [issue-0469, issue-0465, issue-0445, issue-0196]
---

## Finding

`ManagedProcess::wait_for_output_pattern(pattern, timeout)` reads as an
assertion — "wait until this marker appears" — and every caller uses it that
way. It is not one. On timeout:

```rust
// packages/testing/nros-tests/src/process.rs
if start.elapsed() >= timeout {
    self.handle.stdout = stdout;
    self.handle.stderr = stderr;
    if output.is_empty() {
        return Err(TestError::Timeout);
    }
    return Ok(output);          // <- pattern never matched
}
```

The pattern is consulted only for the EARLY-EXIT path. If the timeout expires
and the process printed **anything at all** — including the error explaining why
it failed — the call returns `Ok`.

So `wait_for_output_pattern(MARKER, …)?` means "the process was not completely
silent", not "the marker appeared".

## How it was found

Writing `port_templates_e2e` (issue 0469) for a bug whose signature is known
(0465: the node dies at startup printing `Transport(InvalidConfig)`). The test
was checked against the broken fixture before being trusted — and **passed**.
The failing node's error line is non-empty output, so `Ok`.

Had that check been skipped, the repo would have gained a test that looks like
it guards phase 209's acceptance and cannot fail — the same class as the gate
whose coverage is narrower than its rule (0196), and the verdict that absorbs
the result behind it (0445).

## Blast radius

```console
$ git grep -c wait_for_output_pattern -- packages/testing/nros-tests/tests | awk -F: '{s+=$2} END {print s}'
283
$ # sites that ignore the returned string and check only the Result:
233
```

**233 of 283 call sites never look at the returned text.** Not all are wrong:
many are genuine readiness waits ("wait for `Spinning`, then do the real
assertion"), where a lenient wait is harmless and the test still fails later.
But the ones where the marker IS the assertion — e.g.
`action_multigoal.rs:75` waiting 60 s for `MULTIGOAL_SUMMARY_PREFIX`,
`bridge_*` waiting for a forwarded-count line — are vacuous today, and no
inspection distinguishes the two categories without reading each test's intent.

## Fix direction

The honest contract is that a function named `wait_for_output_pattern` returns
`Err(Timeout)` when the pattern did not appear; the `output.is_empty()` special
case is the defect. That is a one-line change, and deliberately NOT made here:
flipping it converts every vacuous assertion into a failure at once, and those
failures are a mix of

* stale markers (the phase-277 class — the fixture prints something else now),
* genuine product regressions that have been hiding, and
* readiness waits that were always meant to be lenient.

Separating those is the work, and it wants its own pass with the tier green
before and after — not a flag day inside another fix. Sequence:

1. land the contract change behind a temporary strict variant,
2. migrate the readiness waits to the lenient one explicitly,
3. flip the default and triage what turns red,
4. delete the temporary variant.

Step 3 is where the value is: every red is either a test that never tested
anything or a bug that has been passing CI.
