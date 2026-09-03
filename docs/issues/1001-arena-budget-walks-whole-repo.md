---
id: 1001
title: "`check-action-client-arena-budget` walks the whole repo, so `check fast`
  costs minutes on a cold page cache — the pre-push gate is where that is felt"
status: open
type: bug
area: tooling, ci
related: [0900, 0981, 0996]
---

`check fast` promises BUILDLESS and SOURCE-FREE, green in ~23 s. It is what a
contributor runs before every push, and the `pre-push` hook runs it alone. One
gate on that line traverses the entire repository.

## What it does

`check-action-client-arena-budget` answers a POST-LINK question — `nm` over
built cross ELFs — so it cannot use `git ls-files`: the artifacts it needs are
untracked by construction, which its own text carves out ("scanning for
UNTRACKED artifacts is fine — scope it to a build dir"). But the call site does
not scope it to a build dir:

```python
build_root = os.environ.get("NROS_BUILD_ROOT") or os.path.join(ROOT, "build")
roots = [ROOT]                     # ← the whole checkout
```

`PRUNE` drops `.git`, `.fingerprint`, `incremental`, `deps`, `third-party` and a
few others — but not cargo `target*/` dirs, not `zephyr-workspace/`, not `tmp/`.
So the walk is the repo.

## The cost is I/O, not CPU — which is why it hides

Measured on one developer tree (`build/` 79,523 dirs, `target/` 44,382,
`zephyr-workspace/` more than 120 s merely to *count*):

| subtree | dirs traversed | cold | warm |
| --- | ---: | ---: | ---: |
| `examples/` | 98,617 | >60 s (capped) | 0.6 s |
| `zephyr-workspace/` | 49,943 | 60 s (capped) | 0.3 s |
| `build/` | 27,807 | 4.4 s | — |
| `tmp/` | 3,232 | 10.3 s | — |
| `esp-idf-workspace/` | 4,192 | 6.9 s | — |

**The same traversal is 100× apart cold vs warm.** Warm, the whole gate runs in
3.4 s and is unremarkable. Cold — the first `check fast` after boot, or after
another workload has evicted the page cache — it is minutes. On the host this
was found on, with an unrelated build at load 26, the gate did not finish in
600 s.

That is exactly the condition a contributor hits: `check fast` is often the
first thing touching these trees in a session.

It also reports on scratch: a run on this tree printed `[not examined]` lines
for `tmp/scaftest/…`, `tmp/wsx/…`, `tmp/wsx2/…`.

## Two fixes were tried and are WRONG — do not repeat them

**1. Move the gate to the build tier.** Rejected by `check-gate-visibility`,
correctly:

```
action-client-arena-budget: in a lane no pull_request or merge_group job runs
A gate no pull request runs is a gate that rots (issue 0981)
if it passes with no build artifacts it does not belong in the build tier at all
```

The gate SKIPS cleanly (rc 78) when there is no built image, so it passes in a
pristine worktree — which is precisely the test that gate names, and it means
the fast line is its correct home. Relocation is not available.

**2. Prune non-`out` children of `build/<pkg>-<hash>/`.** The anchor is only
ever recorded for a dir named `out` whose grandparent is `build`, so pruning
siblings looked provably free, and it cut `examples/` from 98,617 to 77,387
dirs. It **silently dropped 159 of 410 findings**. The predicate fires on ANY
directory whose parent is named `build` — including the top-level `build/`
shared cargo group dirs (phase-340), amputating whole target trees. Caught only
by diffing a full `--verbose` run against a captured baseline.

Anyone touching this must capture `--verbose` output before and after and diff
it. The gate reports the same *shape* while examining a smaller set, so a
smoke test cannot see the difference.

## Also measured, and NOT a defect

`config-knob-census` and `check-feature-contract` were first blamed at 74 s and
93 s. Both figures were contention artifacts — they were timed underneath a
`-P24` fast-line run on a loaded host. Clean, they are **1.2 s** and **0.43 s**,
and both are index-based (`git ls-files`) and source-only. Recorded because
those wrong numbers are what a careless re-measurement reproduces.

## The design question

Scoping `roots` to discovered cargo target dirs instead of `[ROOT]`, without
losing an image the gate currently finds. Open questions:

1. **What is the root set?** Target dirs here are `<repo>/target`, per-leaf
   `examples/**/target*`, the phase-340 shared group dirs under `build/`, west
   build dirs under `zephyr-workspace/build-*`, and `esp-idf-workspace/`. Is
   that set derivable (from `fixtures.toml` coordinates / `nros_fixture_row_
   artifact_dir`) or does it become a second hand-maintained list that drifts?
2. **Does enumerating them cost what walking them costs?** The dirs are found
   by walking; the saving comes from not descending their interiors, and the
   interiors are where the anchor lives. The win may be smaller than it looks
   and should be MEASURED cold, not assumed.
3. **Should `tmp/` and `test-logs/` be pruned?** Cheap and removes noise, but it
   is a semantic narrowing: an image built in `tmp/` stops being checked.
4. **Is a cold-cache cost acceptable at all on the pre-push line?** The
   alternative framing is that any gate reading untracked artifacts is
   inherently cache-hostile, and the fast line's "buildless and source-free"
   claim (asserted by `check-lane-contracts`) is already false for it.

Tracked as phase-413 W7.
