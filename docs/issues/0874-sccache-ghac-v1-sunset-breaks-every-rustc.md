---
id: 874
title: "sccache 0.8.2 speaks a GitHub cache API that no longer exists — and
  because it is the `RUSTC_WRAPPER`, that fails every `rustc`"
status: open
type: bug
area: ci, tooling
related: [issue-0871, phase-395]
---

## Symptom

`main` and every open pull request fail the required check at once:

    sccache: error: Server startup failed: create gha cache failed:
      ConfigInvalid (permanent) at Builder::build => ACTIONS_CACHE_URL not found,
      maybe not in github action environment?
    error: process didn't exit successfully:
      `/usr/local/bin/sccache …/rustc -vV` (exit status: 2)

`main`'s own `pr-checks` was green at 2026-08-28 16:57 and failing by 2026-08-29
02:22, with no change to the sccache configuration in between.

## Cause

Two facts that only bite together.

**The image ships sccache 0.8.2** (`ci/docker/ci-base/Dockerfile`,
`ARG SCCACHE_VERSION`), released 2024-09-28. GitHub replaced the Actions Cache
service with **v2 on 2025-02-01** and sunset v1 the same day. 0.8.2's GHA
backend only knows the v1 variable `ACTIONS_CACHE_URL`, which no longer exists —
so its backend is permanently unavailable, not intermittently.

The workflow already exports the *correct* v2 variables (`ACTIONS_RESULTS_URL`,
`ACTIONS_RUNTIME_TOKEN`), matching the current upstream docs. The configuration
was right; the binary was too old to read it.

> **CORRECTION, 2026-08-30 — the second sentence was false, and it kept this
> issue open for two days after the bump appeared to close it.** The workflow
> named the right variables but could not read them:
>
>     export ACTIONS_RESULTS_URL="${ACTIONS_RESULTS_URL:-}"
>
> is self-referential. The runner injects those two variables into **JS action**
> contexts, never into `run:` shell steps, so absent from the shell environment
> that line exports the EMPTY STRING. After the 0.17.0 bump the error text
> merely changed — `ACTIONS_CACHE_URL not found` became `cache url for ghac not
> found` — and the backend stayed permanently unavailable.
>
> Measured on run 33293963079: `check-compile-smoke` 104 s and
> `Build nros CLI` 74 s, i.e. **178 s of 396 s (45%) compiling uncached on
> every pull request**, behind a `::warning::` and nothing else.
>
> `actions/cache@v4` is the proof rather than a guess: it is a JS action, it
> needs the same two variables, and it succeeds in this very job.
>
> Fixed by exporting them through `actions/github-script` into `$GITHUB_ENV`.
> And the reason nobody saw it: the `sccache --show-stats` step — whose own
> comment says a silently-missing cache "looks exactly like one that is
> working; the only difference is in this output" — did not list
> `pull_request`, so the event that compiles on every push had no stats. Both
> the export and the diagnostic excluded the case they existed for.

**And sccache is the `RUSTC_WRAPPER`** — the justfile sets it whenever the
binary is on PATH. So a cache backend that cannot start does not degrade to an
uncached build: it prefixes every `rustc` invocation with a command that exits
2. A cache failure became a compiler failure.

Note the asymmetry that made this worse than it needed to be: an **absent**
sccache was already handled gracefully (`::warning::… runs UNCACHED`), while a
**broken** one was fatal. The graceful path existed and the failure took the
other one.

## Fix

Both halves, because either alone leaves a hole.

1. **Bump to sccache 0.17.0** (latest; 2026-07-29), in `ci/docker/ci-base/
   Dockerfile` and `nros-sdk-index.toml` together — the Dockerfile comment
   already requires those to move in lockstep, or `nros setup --tool sccache`
   provisions a different sccache than CI runs. ghac v2 landed upstream in
   0.10.0 ("Bump opendal to 0.52 to support ghac v2") and moved to v4 in 0.11.0,
   so 0.10.0 is the floor and this takes the current release.

2. **Prove the backend before handing it the compiler.** The workflow step now
   exports the GHA config into itself, runs `sccache --start-server`, and
   publishes to `$GITHUB_ENV` only if that succeeded. On failure it warns and
   leaves the config unset, so sccache falls back to its local disk cache —
   the same uncached-but-working state as the absent case.

The second is the durable part: it makes ANY future backend breakage cost a
cache rather than a build, which is what should have happened this time.

## Measured

On the maintainer host, against the 0.17.0 release tarball:

* the Dockerfile's constructed URL resolves and the binary runs
  (`sccache 0.17.0`); asset naming is unchanged from 0.8.2.
* GHA enabled with no cache vars — the CI condition — exits **2**, so the new
  guard detects it. Its message no longer names `ACTIONS_CACHE_URL`, confirming
  the newer backend.
* the same environment with GHA left unset exits **0**, and `sccache rustc`
  compiles and runs a test program — so the fallback is genuinely working, not
  merely starting.

NOT measured, and stated rather than implied: that 0.17.0 talks to GitHub's
live cache service successfully. That needs a real Actions runner, and the only
place it can be observed is CI itself. If it cannot, the guard now makes the
result an uncached build rather than a red tree.

## Acceptance

* The required check passes on `main`.
* A cache backend that cannot start costs the cache, never the compiler —
  verified by the warning appearing with the build still green.
