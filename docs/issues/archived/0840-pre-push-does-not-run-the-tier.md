---
id: 840
title: "Nothing between a green local edit and `origin/main` asks whether a tier
  was ever run, so four separate reds landed on main in one day"
status: resolved
resolved_in: "issue-0840 (this filing)"
type: tech-debt
area: build
related: [rfc-0061, phase-318, issue-0833, issue-0743, issue-0319]
---

## Problem

On 2026-08-27, working through an unrelated queue, I hit **four** independent
reds on `main` in a single session. None was subtle, none was expensive to fix,
and every one of them was already covered by an existing gate:

| Red | Landed in | Gate that covers it | Tier |
| --- | --- | --- | --- |
| `nros-rmw-zenoh` lib test target does not compile (E0369/E0277) — **all 69 tests dead** | `1d1dc4bf3` | `check-test-targets` (`clippy --all-targets -D warnings`) | build |
| `keyexpr.rs` committed unformatted | `1d1dc4bf3` | `check-workspace-fmt` | fast |
| profile literal in `isotp-pico-interop.sh` | `3770510f4` | `check-build-profile-literals` | fast |
| hand-rolled `export RMW_IMPLEMENTATION=` in `isotp-ros-interop.sh` | `3770510f4` | `check-ros-env-spelling` | fast |

Two commits, four gates, zero of them run. `just check` would have caught all
four. So the defect is not a missing gate and not missing diligence about
*which* gate — it is that **nothing between the edit and the push asks whether
any tier ran at all.**

## Why this is a WHEN problem, and why the hook is the place

`.githooks/pre-push` already argues this exactly, for the issue-id check:

> Why here and not only in `check-fast`: the gate has always been able to catch
> this, and repeatedly did not, because of WHEN it runs.

The two checks it carries today (duplicate issue id, submodule pin rewind) share
a shape: the defect becomes true **at rebase**, so it is invisible to any check
the author ran before. These four are a different shape — they were true in the
author's own tree the whole time, and simply went unexamined.

That difference matters for the design. A re-check of what the rebase changed
would not have caught any of these. What catches them is running the gates,
once, at the last moment where refusing is still cheap.

## Cost, and why the obvious answer is wrong

The obvious fix — run `just check` on pre-push — is the one CLAUDE.md already
warns against, in its own words about the pre-phase-318 `just ci`:

> an instruction nobody could afford per task, so it got followed selectively,
> which is worse than a smaller instruction followed honestly

`check` includes `check-build`, which compiles the workspace. A pre-push hook
that costs minutes gets `--no-verify`'d within a week, and then the guarantee is
worth less than none, because its presence implies coverage nobody has.

`check-fast` is the affordable half. It is buildless and source-free by
construction, and it covers **three of the four** reds above. The fourth needs a
compile, and it is honest to say so rather than pretend the hook is complete.

## Fix

Run `just check fast` from `pre-push`, on branch pushes only, alongside the two
checks already there.

Deliberate limits, stated so nobody reads the hook as more than it is:

* **Three of four, not four of four.** The compile-tier red (`check-test-targets`)
  stays outside. Catching it needs `check-build`, which is not affordable here.
  The hook says so when it passes.
* **One existing recipe name, not a curated subset.** A hand-picked gate list in
  the hook is a second spelling that drifts from `check-fast` silently — the
  `#282`->`#326` shape, and the same one issue 0833 was.
* **`--no-verify` still works.** A hook that cannot be bypassed gets deleted
  instead.

## Measured

**CORRECTED 2026-08-27, after the hook shipped.** The number first recorded
here was 63-64 s, from two consecutive `just check fast` runs on an idle host,
described as "stable". It is right, and it is the wrong measurement.

`check-fast` costs 64 s on a SETTLED tree. The hook does not run on a settled
tree: it runs immediately after a rebase, because that is what a push follows.
Timed in that condition — the real one — a push took **267 s**. A `check-fast`
run on the same tree minutes later was 64 s again, so the 4x is the rebase, not
load: a rebase rewrites source mtimes, and the gates that shell out to cargo
re-fingerprint everything they touch.

**SECOND CORRECTION, same day: the MECHANISM above is also wrong.**

"a rebase rewrites source mtimes, and the gates that shell out to cargo
re-fingerprint" was asserted, not tested. Tested now, by varying exactly that
one variable — `touch` every tracked `.rs/.c/.h/.cpp/.hpp/.toml`, which is
precisely what a rebase does to mtimes, and re-run:

    settled tree   64 s
    after touch    81 s      (+17 s)

Not +200 s. And the +17 s is almost entirely ONE gate, which goes from ~1 s to
~19 s; every other gate moves by a second or less. So mtime-sensitivity is real,
small, and localised — it is not what made that push cost 267 s.

The 267 s is a real measurement and its cause is now UNKNOWN. The likeliest
remaining candidates, neither tested: contention (that push followed a heavy
fixture build), or CONTENT changes from pulling 26 commits, which invalidate
cargo fingerprints by hash rather than by timestamp — a different mechanism from
the one I named.

So the follow-up this issue asked for is done and returned a negative result:
there is no cluster of mtime-hungry gates to fix. Anyone narrowing the hook on
the strength of the 4x figure would be optimising against a cause that has not
been demonstrated. What IS established: the hook costs ~64 s on a settled tree,
~81 s after an mtime-only disturbance, and once cost 267 s under conditions not
yet isolated.

Three errors in one issue, all the same shape: a number measured under one set
of preconditions, then explained with a mechanism nobody varied. The first
correction fixed the number and introduced a story; this one removes the story.

This is phase-371's lesson repeating, in the same session that recorded it:
*repeating a measurement is not varying its preconditions.* Two runs agreeing
tells you the variance is low; it tells you nothing about whether you measured
the case that matters. Both runs shared one precondition — a settled tree — and
that precondition is exactly the one the hook never has.

**So the affordability argument this issue makes for itself does not hold.**
4.5 minutes per push is the "gets `--no-verify`'d within a week" range this
issue explicitly designed against. The mechanism is correct and stays; the
COSTING was wrong, and a follow-up owes either a per-gate breakdown of what the
rebase actually re-fingerprints (a small number of gates likely dominate) or a
narrower trigger. Until then the hook is honest but more expensive than
advertised, and `--no-verify` is the documented escape.

Measured AFTER the tier-2 fixture build finished, deliberately. An earlier
timing in this repo (phase-371's 481 s gate phase) was taken under contention
and had to be retracted at ~90 s; the same mistake was available here, with a
build running at load 8.8.

Sixty seconds per push is the affordability argument. It is enough to notice and
not enough to route around.

## Verification

Each path exercised, not inferred:

* **Red tree refuses.** A tracked file carrying `target/release/examples` makes
  `check-build-profile-literals` fail; the hook exits 1 with the refusal text.
  (Probe file removed; tree clean afterwards.)
* **Bypass works.** `NROS_SKIP_PREPUSH_CHECKS=1` skips and says so;
  `git push --no-verify` skips the hook entirely.
* **Non-branch pushes stay free.** A `refs/issue-ids/*` push returns in ~0 s
  with no gates run — which also preserves the property the issue-id check was
  fixed for: `reserve-issue-id.sh` must be able to push its reservation ref
  while the tree is red, or the tool that resolves a collision is blocked by it.
* **Missing `just` does not block a push.** Absent runner prints a note and
  exits 0: a hook that fails on its own tooling gets deleted.

## Not fixed here

The compile tier. If these keep landing, the next step is not a bigger hook —
it is CI on the push, or a tier-run stamp the hook can check for cheaply
(`tree hash + tier + verdict`), which moves the cost back to where it belongs:
paid once by whoever was supposed to run the tier.
