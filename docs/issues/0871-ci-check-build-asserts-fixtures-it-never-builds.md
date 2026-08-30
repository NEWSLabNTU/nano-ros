---
id: 871
title: "Every PR is red on a fixture CI never builds — and `main` cannot see it,
  because the required gate does not run on push"
status: open
type: bug
area: ci, testing
related: [issue-0196, issue-0034, issue-0837, phase-395]
---

## Symptom

Every open pull request fails the required status check
`check (fast on push; full on PR/nightly)`:

    ---- platform_headers_compile_per_capability stdout ----
    Error: BuildFailed("Test fixture binary not prebuilt:
      /__w/nano-ros/nano-ros/build/compile-check-fixtures/platform_hdr_posix_cpp_heap/.compile-ok
      Run `just build-test-fixtures` first.")
    error: recipe `check-source-gates` failed with exit code 101

Observed on PRs #7, #8, #10 and #12 — different authors, unrelated diffs. None of
them touches the compile-check fixtures.

## Cause

`check-source-gates` runs three `cargo test`s that assert a prebuilt
`.compile-ok` stamp (issue 0034's "no compilation inside tests" — the compile
moves to the build stage and the test asserts the artifact). It is a dependency
of `check-build`, which `check` depends on.

`pr-checks.yml` runs that job with **no fixture build of any kind**. The
fixture-owning lane is a different workflow (`host-tests.yml`'s integration job,
which does build fixtures). So the gate asserts an artifact its own job never
produces.

## Why `main` stays green

The build tier is gated on `github.event_name != 'push'`:

    - name: just check build + no_std
      if: ${{ github.event_name != 'push' && !cancelled() }}

So a push to `main` stops after `check-fast` and never reaches
`check-source-gates`. Only pull requests run it, and they all fail. The branch
the gate protects is the one branch that never runs it — which is why this could
sit red across every PR while `main`'s own CI reported success.

That asymmetry is the issue-0196 class stated one level up: the build side and
the test side disagreeing about what exists, with nothing that compares them.

## Fix

`build-compile-check-fixtures` — a standalone recipe for the small subset
`check-source-gates` actually asserts — is now a CI step ahead of `check-build`.
Not `build-test-fixtures`: that builds the whole matrix and this job cannot
afford it. `build-test-fixtures` calls the new recipe, so there is ONE spelling
and the two callers cannot drift.

Measured: 428 s warm on the maintainer host (36 rows, 5 builders, only the 4 px4
rows rebuilding). Cold in CI will be more; if that proves too slow the next move
is to narrow the row set to what `check-source-gates` asserts rather than to drop
the step.

## What this does NOT fix

The structural asymmetry remains: `check-build` still runs on PRs only, so any
future fixture-dependent gate added to it will be invisible to `main` in exactly
the same way. Two candidates, neither taken here:

* run the build tier on push too (costs every push what it costs a PR), or
* have `check-source-gates` declare its own fixture prerequisite, so the recipe
  is self-sufficient wherever it runs and cannot be placed in a fixtureless job
  by accident.

The second is the issue-0196-shaped fix; it was not taken now because it slows
every local `just check` for people whose fixtures are already built.

## Acceptance

* A PR touching nothing fixture-related passes the required check.
* A fixture-asserting gate cannot be added to a job that builds no fixtures
  without something failing loudly — or the gap is written down where the next
  person adding one will read it.
