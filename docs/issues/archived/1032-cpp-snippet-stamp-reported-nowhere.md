---
id: 1032
title: "a compile-check snippet's failure was reported at neither end: the build stage defers to the consuming test, the consuming test skipped unconditionally, and one snippet had no consumer at all"
status: resolved
area: testing, build
severity: medium
found: 2026-09-04
resolved: 2026-09-04
related: [1031, 1030, 0034, 0309]
---

# "Does not fail here because it fails there" — failing nowhere

## The chain

Three `cxx-syntax` snippets failed in every scheduled `gate.yml` run from at
least 2026-09-01 (issue 1031 is why they failed). Nothing went red, at either
end of a two-step handoff:

1. **Build stage** — `cxx_syntax_check` on a failed compile echoes
   `cxx-syntax FAILED for <id> (no stamp; consuming test will report)` and
   returns. The step exits 0. This is deliberate: a snippet that will not
   compile must not block `build-test-fixtures`. Its comment states the
   contract — "the consuming test reports the gap per tier (hard-fail full /
   [SKIPPED] light)".

2. **Consuming test** — `cpp_api_drift.rs`'s `assert_snippet_compiled` caught
   `Err(_)` from `require_compile_check` and called `nros_tests::skip!`
   **unconditionally**. A skip is not a failure. No tier logic, despite the
   claim above.

So step 1 deferred to step 2, and step 2 reported nothing.

The tier policy the build script described was not missing from the codebase —
it lives in `require_compile_check`, which is tier-aware (hard-fail in the full
tier, `[SKIPPED]` under `NROS_FIXTURES_OPTIONAL=1`). `cpp_api_drift.rs` threw it
away by catching the error. The sibling consumer,
`platform_header_compile.rs`, has always used `?` and has always failed
correctly.

## The third snippet

`spin_until_future_complete` was asserted by **nothing**. A search for its id
across the test tree found only the fixture file. It was one of the three
failing nightly and the one no consumer could ever have reported — a step worse
than a skip that says nothing.

## Stale justification

The skip cited issue 0034 as tracked pre-existing drift. 0034 was resolved and
archived on 2026-06-12. Its named cause here — "`rclcpp_node_options` needs
generated config headers" — was issue 1031, fixed the same day as this. The
excuse outlived the defect by three months, and the module doc still described
the snippets as currently failing.

## Fix

* `assert_snippet_compiled` propagates with `?`, restoring the tier policy
  rather than writing a second one.
* `spin_until_future_complete_compiles` added.
* The build script's comment no longer asserts what another file does without
  qualification; it says the deferral is only safe while a consumer exists.
* **Structural**: `fixtures-manifest.py validate-compile-checks` now rejects a
  `cxx-syntax` row whose id no test names. It runs in the fast tier via
  `check fixtures-manifest`, so a snippet added without an assertion cannot
  land. All 12 current rows pass with no baseline.

## Verified

* All four tests pass with the stamps present.
* Removing `spin_until_future_complete`'s stamp turns the test RED (it was
  silently green before).
* Under `NROS_FIXTURES_OPTIONAL=1` the same missing stamp yields `[SKIPPED]`,
  so the light tier is unchanged.
* Mutation on the new gate: repointing the new test at another id makes
  `validate-compile-checks` fail naming `spin_until_future_complete`.
