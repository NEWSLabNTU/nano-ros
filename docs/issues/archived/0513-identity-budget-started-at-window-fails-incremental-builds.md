---
id: 513
title: "`check-artifact-identity-budget`'s `started_at` window fails any INCREMENTAL fixture build — cargo does not rewrite an rlib it did not have to rebuild"
status: resolved
resolved_in: phase-340
type: bug
severity: high
area: build
related: [issue-0499, issue-0485, phase-340]
---

## Resolution (2026-08-11) — option 2, because it cannot produce a false green

When the era window is non-empty but holds NO artifact of the budgeted crate,
the gate now measures the WHOLE tree and labels the reading as
possibly-historic, instead of hard-failing.

That direction was chosen over reading `.fingerprint/` (option 1) for a reason
worth recording: the fallback can only count **more** artifacts, never fewer, so
it cannot turn a real over-budget into a pass. Verified rather than assumed — with
the budget and ceiling forced to 1, the fallback path still FAILS and prints the
full identity list. A `.fingerprint/`-based liveness test would be more precise
and is still the better long-term answer, but it can under-count if the mapping
is wrong, and under-counting here is silent.

0499's two existing behaviours are untouched and were re-verified:

* every rlib predating `started_at` → still `[SKIP] … this tree is history`
* filter active and the crate present → still the strict, filtered count

### The advice was wrong too

The `else` arm of `era_verdict` said *"This stamp has no started_at"* — but in
this case the stamp HAS one; the filter simply could not speak for this crate.
It sent the reader looking for a missing stamp sitting right in front of them.
Now a third arm says what actually happened: *this build did not rebuild
`nros_core`, so the count above is UNFILTERED*.

### Self-tested, standing

The fallback predicate is checked on every run, like the collation counter
beside it, and for the same reason: **the bug is invisible in output.** The gate
printed a confident, specific, wrong `NONE for nros_core` and exited 1 on a
correct tree. A predicate that silently stopped firing would restore exactly
that without looking any different. Tripwired: breaking the predicate makes the
gate fail with the self-test message.

### Verified

* the reproducer (window holding one non-`nros_core` rlib) — FAILS before, PASSES after
* over-budget under the fallback still FAILS, with the history caveat
* all-history tree still SKIPs
* `bash -n` clean

## Symptom

`just ci` (tier 1) fails at its first gate, on a green fixture build:

```
artifact-identity budget: FAIL
  counted 16 of 244 rlib(s) — those written since started_at=2026-08-11T01:02:20Z (issue 0499).
  examples/workspaces/mixed/build-workspace-fixtures holds compiled rlibs, but NONE for nros_core.
  The gate cannot answer the question it exists to ask, so it fails
  rather than passing on a tree it did not understand.
```

The build immediately before it succeeded (`Native test fixtures built.`), and
the tree does hold `nros_core` rlibs — four of them:

```
started_at = 2026-08-11T01:02:20Z          <- this build
libnros_core-*.rlib   00:13:32             <- the PREVIOUS build, ~50 min earlier
libnros_core-*.rlib   00:14:06
libnros_core-*.rlib   00:14:10
```

## Cause

The issue-0499 fix counts only rlibs written since the fixture build's
`started_at`. That correctly excludes accumulated history — which was the point,
and it works. But it also excludes **everything cargo did not have to rebuild**,
and on an incremental build that is nearly everything.

A run whose diff does not reach `nros_core` leaves its rlibs untouched, so zero
fall inside the window, and the gate's "no rlibs for the budgeted crate" arm —
written for a partial build or a renamed crate — fires on a tree that is
complete and correct.

So the gate now fails **the normal case**: any tier-1 run that rebuilds only
what changed. Reproduced by running `build-test-fixtures lane=native` twice; the
second run's `started_at` postdates every `nros_core` artifact.

## Why this matters more than the count being wrong

`check-artifact-identity-budget` is the FIRST member of `check-fast`, and `ci`
stops at the first failure, so the whole build tier, clippy and `test-all` never
run. Issue 0499 was filed because a stale tree could block tier 1 this way; the
fix replaced "fails on an accumulated tree" with "fails on an incremental one",
which is more common, not less.

The prescribed remedy is also the expensive one: `rm -rf` the workspace build
dir and rebuild, minutes of work to make one crate's mtime recent.

## Direction

The window is the right idea applied to the wrong question. "Which identities
did THIS build produce" needs the era filter; "how many identities does this
crate have" does not — an rlib cargo chose not to rewrite is still a live
identity of the current tree.

Options, roughly in order:

1. **Filter by era only when deciding what is ACCUMULATION.** An rlib whose
   fingerprint cargo still considers current belongs to the tree regardless of
   mtime. Cargo's own `.fingerprint/` dir is the authority the gate could read
   instead of mtimes.
2. **Fall back to the whole tree when the window is empty for the budgeted
   crate**, and say so — a possibly-inflated count that the reader can act on
   beats a hard failure on a correct tree.
3. **Key the era on the fixture stamp's `built_at` of the LAST build that
   touched this workspace**, not on the current run's `started_at`.

Whichever: the gate must not fail on a tree it built correctly a moment ago.
Issue 0499's own closing line applies to its fix as well — *a gate whose red
requires a rebuild before it can be believed is a gate whose red will be
ignored.*

## Reproduce

```sh
source ./activate.sh
just build-test-fixtures lane=native      # populates the tree
just build-test-fixtures lane=native      # incremental: nros_core not rewritten
bash scripts/check-artifact-identity-budget.sh
```

Fails on the second run with `NONE for nros_core`.

## Not caused by the change that hit it

Observed while running tier 1 for issue 0470 (a `nros-tests` + `stress-xrce`
change). Neither touches `nros_core` or the mixed workspace, which is precisely
why its rlibs were not rebuilt.
