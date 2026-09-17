---
id: 1377
title: "A `[SKIPPED]` reason that reaches the junit only in `<system-err>` is
  read as empty, so `check-skip-budget` fails a host for a capability the
  baseline already declares — and the pattern that would match it is sitting in
  the file"
status: resolved
type: bug
area: ci, testing, tooling
severity: medium
related: [1161, 0658, 0584, phase-456]
---

## Symptom

On a host that HAS a ROS zenoh router, `just ci gate` and `just test-unit` fail
at `_check-skip-budget`:

```
check-skip-budget: 1543 ran, 0 deselected (out of lane), 1 skipped for an unmet
  precondition — capability=1
  1 skip(s) name something this host lacks; those are the actionable ones:
         1x
ERROR: 1 test(s) skipped for a capability this lane never declared it may lack:

      first seen: nros-rmw-zenoh::zenoh_integration two_sessions_deliver_cross_session_through_router
```

The reason is blank, and `.config/capability-skip-baseline.txt` already contains
the line that would have matched it:

```
second session refused — shim built with ZPICO_MAX_SESSIONS=1
```

## Why it only shows up on some hosts

The test early-returns when no router is found, so a host without one never
reaches the skip. With a router present it opens a session, is refused a second
one (the shim's default is `ZPICO_MAX_SESSIONS=1`) and takes the
`nros_tests::skip!` — which is the correct behaviour and exactly what the
baseline declares.

## Cause, measured

Three defects in a row, each masking the next.

**1. nextest emitted no message and no body.** For this invocation the raw junit
carries `<failure>` with `message=None` and `text=None`; the panic text reaches
the file only in `<system-err>`. `rewrite-skipped-junit.py` already knows this
can happen — it reads the streams to CLASSIFY the skip — but it writes
`msg = f.get("message") or SKIP_MARKER`, so the rewritten `<skipped>` carries the
bare `[SKIPPED]` and the reason is left behind on the testcase.

**2. `SKIP_LINE` matched a bare marker with a space as its "reason".** The
pattern captured `(.+?)`, so `[SKIPPED] ` matched with a single space, the
extractor reported that as the reason, and no fallback ran.

**3. `\s*` crossed the newline.** Once the streams were consulted, the pattern's
separator was `\s*`, and `\s` matches `\n` — so a bare `[SKIPPED]` on one line
swallowed the line break and captured the NEXT line, reporting
`thread 'n' (1) panicked at a.rs:1:1:` as the reason. A plausible wrong string,
which is worse than an empty one.

The classed marker `[SKIPPED:<class>]` was also unmatched by the same pattern —
the bare-spelling defect issue 0658 fixed in the Rust aggregators and in the
rewriter, surviving here in the consumer.

## Fix

In `scripts/test/check-skip-budget.py`:

* `skips()` appends the testcase's `<system-out>`/`<system-err>` bodies, via the
  shared `skip_marker.testcase_streams`, when the payload carries no reason —
  so the reason is read from where it actually is;
* `SKIP_LINE` accepts the classed marker, uses `[^\S\n]*` as its separator so it
  cannot cross a line, and requires the reason to begin with a non-space
  character.

Four self-test cases cover it, including one that builds the exact junit shape
(`<skipped message="[SKIPPED]">` plus the panic in `<system-err>`) and asserts
the reason is recovered. The self-test runs on the normal path.

## What it was hiding

`check-skip-budget` is the gate issue 1161 added so a capability skip cannot
pass silently. With the reason unreadable, its baseline could never be consulted
for skips of this shape: every one of them failed the lane regardless of what the
file declared, and the error named the test rather than the missing capability.
The gate was strictly worse than useless for that class — it failed closed on
exactly the hosts that had the most provisioning.
