---
id: 993
title: "Forty gates sat in a lane no pull request runs, and the lane's own
  comment claimed otherwise"
status: resolved
type: bug
area: ci
severity: medium
found: 2026-09-02
related: [issue-0981, issue-0952, phase-395, phase-396]
---

## Symptom

Issue 0981's `codegen_golden` sat red on `main` for a day while the required
`CI` context stayed green, and two separate changes reached a green pull request
while failing `just ci gate` locally. Both were in `check::build`.

## Measured

`just check build` is invoked in exactly one place:

```
.github/workflows/gate.yml:678
  - name: just check build + no_std (nightly / manual only)
    if: ${{ contains(fromJSON('["schedule","workflow_dispatch"]'), github.event_name) ... }}
```

So **no pull request and no merge group runs any of its 40 gates.** A red there
is invisible until somebody reads a nightly.

And the lane's own comment said the opposite:

> Minutes + source/CLI prereqs; runs on PR + nightly (`default.yml` non-push),
> not on every direct push to main.

That is the shape this repo keeps paying for — a claim nobody checks is a claim
that is eventually false — and here it actively misdirected: anyone reading
`check.just` to decide where a new gate belongs was told the build tier gates a
pull request.

## What actually belongs there

Not a judgement call. `check-lane-contracts.py` already draws the line for
affordability tiers (compile-stage artifacts a job can produce, versus runtime
fixtures it cannot), and the same test works here: **run the gate in a pristine
worktree.**

```
$ git worktree add --detach <tmp> HEAD
$ cd <tmp> && just check <gate>
```

Of 21 candidates that looked pure, **20 pass with no build artifacts at all**,
in 47–1360 ms each. They moved to the fast line: measured 189 gates at -P32,
slowest still 11.6 s — the additions cost no measurable wall clock.

The 21st, `borrowed-e2e`, fails there:

```
borrowed-e2e: building nros-c (platform-posix)…
FAIL: nros-c config header missing at <tmp>/target/nros-c-generated/nros/nros_config_generated.h
```

It compiles `nros-c` and reads what that generates. That is a real reason to be
in the build tier, and the pristine run is what distinguishes it from "nobody
got round to moving it". The heuristic that preceded the test — grep the recipe
for `cargo`/`cmake` — called 28 gates pure and would have moved eight that
delegate their building to a script.

## Fix

* 20 gates moved to `fast`, so they gate a merge.
* The lane comment corrected, and it now says where the lane runs and why.
* `just check gate-visibility` → `scripts/check-gate-visibility.py`, fast line,
  static. The remaining ungated set is written down in
  `.config/ungated-gates.txt` and may only SHRINK — same ratchet as the
  selftest baseline. Each of the 19 entries carries the reason it fails a
  pristine run, grouped by kind (compiles a crate; needs the CLI; needs an RMW
  backend's sources; needs a linked image).

Adding a gate to a non-merge-gating lane now requires adding a line there,
which is the moment to ask whether it needs to be there.

### This gate's first version was vacuous

It reported `0 gate(s) run by no merge-gating job` — i.e. it thought everything
was gated. The guard it exists to read is

```
if: ${{ contains(fromJSON('["schedule","workflow_dispatch"]'), github.event_name) ... }}
```

where the event names are DOUBLE-quoted inside a single-quoted string, and the
event regex matched only `'schedule'`. Seeing no events, it concluded the step
was unguarded. Fixed to accept both quote styles, with that exact guard kept as
a self-test case.

## Left open

The 19 remaining gates still gate nothing on a pull request. Closing that means
either paying their prerequisites in the PR job (`generate-bindings` is measured
at 23 s on a ROS image, but `check::build` as a whole is ~587 s) or splitting
them further. Both are latency decisions about every PR rather than repairs, so
they are not made here — the ratchet makes the set visible and shrinking, which
is what was missing.

## Acceptance

* [x] Every gate that can run without build artifacts gates a merge.
* [x] The lane's documented trigger matches the workflows.
* [x] The ungated set is explicit, reasoned, and may only shrink.
