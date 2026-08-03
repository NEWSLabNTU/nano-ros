---
id: 407
title: Tier 1 runs a test whose fixture its own lane never builds
  (logging_smoke needs a threadx-linux fixture)
status: open
type: bug
area: testing
related: [0196, 0401, 0406]
---

## Problem

`just ci` is defined as

```
NROS_FIXTURE_SCOPE=native NROS_TEST_SCOPE=native NROS_FIXTURE_LANE=native just check rust-rtos-link-check test-all
```

so the fixture LANE and the test SCOPE are both `native`. But the native test
scope selects `logging_smoke::logging_smoke_harness_captures_stderr`, and that
test needs a **threadx-linux** fixture:

```
logging-smoke-threadx-linux fixture not built - run `just threadx_linux build-fixtures`
```

`lane=native` never builds it, so tier 1 fails on a fixture tier 1 declines to
build. Nothing is wrong with either half on its own — the lane is honest about
what it builds and the test is honest about what it needs; they simply do not
agree, and the disagreement is only discoverable by running the tier to
completion.

This is the issue-0196 class ("build-side stale probes must watch the same
inputs as test-side gates"), one level up: it is not the freshness probe that
disagrees, it is the SET.

## Repro

```sh
just build-test-fixtures lane=native
NROS_TEST_SCOPE=native cargo nextest run -p nros-tests \
    -E 'test(logging_smoke_harness_captures_stderr)'
```

## Fix options

1. **Scope the test out of the native lane** — if the intent is that a
   threadx-linux fixture belongs to the threadx-linux lane, the test should be
   selected by the lane that builds it, not by `NROS_TEST_SCOPE=native`.
2. **Add the row to lane=native** — if the logging harness is meant to be
   covered by tier 1, the lane must build it (costs a threadx-linux fixture in
   the cheap tier, which is probably why it is not there).
3. **Make it skip rather than fail** when out of lane — weakest option: a test
   that silently skips in the tier that is supposed to cover it is how coverage
   quietly goes to zero (CLAUDE.md "Tests must fail on unmet preconditions").

(1) looks right, but it is a coverage decision for whoever owns the lane map
(#393 / phase-318).

Whatever is chosen, a gate that cross-checks "every test the scope selects has
its fixture in the lane" would catch the next instance at configure time
instead of an hour into a sweep.

## Notes

Found finishing tier 1 for the issue-0383 work (2026-08-03/04). Distinct from
#0406, which is about eight tests that fail with their fixtures correctly built.
