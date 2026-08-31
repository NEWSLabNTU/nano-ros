---
id: 977
title: "Audit: every `||` in CI and the test scripts, classified — two real
  failure-maskers out of 205"
status: resolved
type: tech-debt
area: ci, testing
related: [0700, 0359, 0584]
---

## Why

`||` in a test or build step is a request to keep trying until something
succeeds, and the step then reports success without saying which command
actually passed. That is the same defect as a skip reported as a pass, and this
tree has now hit it twice — `nightly.yml`'s
`build-all || build-examples || build` (fixed) and issue 0700's west-fixture
wrapper before it.

So: sweep everything, classify every occurrence, and record the taxonomy. The
count is not the finding; **which kind** each one is, is.

## Method

`.github/workflows/*.yml` (parsed, comments and heredoc bodies excluded — a
sentence about a command is not a command) plus every tracked `scripts/**`,
`just/*.just` and `justfile`. 205 occurrences.

## The taxonomy

Four shapes. Only the last is a defect.

**1. Cleanup — `rm … || true`, `apt-get clean || true`.** ~150 sites. The
command is best-effort by nature and its failure carries no information. Fine.

**2. Reporter before a decided failure.**

```
    just _name-real-failures || true
    just _check-skip-budget  || true
    exit 1
```

The exit is already decided; these print diagnostics on the way out, and a
diagnostic that fails must not replace the real error. Fine — and note the
SUCCESS path three lines below calls `just _check-skip-budget` **unswallowed**,
which is the assert. The same file gets both right.

**3. Swallow, then check the OUTCOME instead of the exit code.**

```
    env "${tc_env[@]}" west "${args[@]}" || true
    if [ -e "$bld/$output" ]; then …
```

Stronger than the exit code, not weaker, and `west-fixtures.sh` says why: a
`west-configure` row is *expected* to stop before linking, and a `west-build`
row that exits 0 without its image is not built either. `bump-manifest.sh` does
the same with locks, verifying "every lock names exactly ONE revision" after.
Fine, because the verification is unconditional.

**4. Fallback — the defect.** The left side failing causes something *different*
to run, and the step reports success for whichever worked. Two found:

* `nightly.yml`: `just <plat> build-all || just <plat> build-examples ||
  just <plat> build`, both branches. Fixed separately; gated by
  `check-ci-no-verb-fallback`.
* `justfile`'s `lock-update`: `cargo update --workspace 2>/dev/null ||
  cargo update`. Fixed here.

## The one fixed here

`--workspace` is supported by the pinned cargo (1.97.1), so that fallback could
only fire when the NARROW refresh failed — and a bare `cargo update` is a
whole-graph re-resolve, the operation issue 0359 records as moving 5388 lines in
one "cleanup". It escalated from the safe form to the dangerous one, silently,
inside the recipe that exists so a lock moves only when someone means it. The
`2>/dev/null` hid the reason the narrow refresh failed, which is the one thing
the caller needs.

## What is NOT worth a gate

A blanket ban on `||` would flag ~200 correct sites and be turned off within a
week — the "if the narrow spec is awkward, people reach for a bypass" failure
phase-411 names. Shapes 1–3 are not distinguishable from shape 4 by syntax
alone: `cmd || true` is fine when a verification follows and wrong when the path
continues to success, and no regex knows which. `check-ci-no-verb-fallback`
gates the one shape that IS syntactically decidable — a `just` verb falling back
to another `just` verb — and the rest is this document.

## If you are adding one

Ask which shape it is. If it is 4, do not. If it is 3, put the verification
directly after it and say what it checks. If it is 2, make sure the failure
below is unconditional. If it is 1, nothing to do.
