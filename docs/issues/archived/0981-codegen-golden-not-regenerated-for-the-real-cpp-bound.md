---
id: 981
title: "The RX bound had TWO hand-derivations, and the fix that landed caught
  one — the other passes only because its type happens to be 4-aligned"
status: resolved
type: bug
area: codegen, ci
severity: medium
found: 2026-09-01
related: [phase-408, issue-0896, issue-0964, issue-0952, issue-0088]
---

## Symptom

`just ci gate` failed at step 3 of 6 (`check::build`, via `check-cli-tests`):

```
configured/Bounded.hpp   golden RX = 133   now 136
configured/Capped.hpp    golden RX = 157   now 160
inline/Bounded.hpp       golden RX = 133   now 136
```

and, one layer down, `message_size_bound_parity` asserting `Some(133)` against
an emitter producing `Some(136)`.

## Measured

Reproduced in a `git worktree` freshly checked out at `origin/main`, then
bisected by running the one test per revision in that clean tree:

```
07748a644  ok       feat(phase-408 W5b): the info and validated C subscriptions size their arena
5f3c08545  FAILED   feat(phase-408, #0896): the C++ pack emits the REAL bound, and spends it
```

Adjacent revisions — `07748a644` is `5f3c08545`'s parent, so no interval was
left to search.

## Root cause: two derivations of RX, 65 seconds apart

`ec63d4ed9` (19:11:10Z) introduced `transport_framed`, so
`rx = next_multiple_of_4(max(xcdr1, xcdr2))` — a receive buffer holds what the
transport DELIVERS, and one sized to `n` exactly refuses the framing bytes
rather than truncating them, losing the message.

`5f3c08545` (19:12:15Z) taught the C++ pack to emit the bound and added
`message_size_bound_parity.rs`, whose expectation is computed as `a.max(b)` —
the pre-framing rule. The golden was written from that same expectation, so the
test and the golden agreed with each other and disagreed with the SSoT, which
is why the symptom was an arbitrary-looking 3 bytes rather than anything
naming a rule.

That is the defect the same commit's message says it restructured the emitters
to avoid:

> **ONE DERIVATION, TWO LANGUAGES.** […] the block MOVED:
> `generator::common::derive_message_bound` is now the only place a message's
> `(tx, rx, reason, poison token)` is computed

The emitters were consolidated; the test then re-derived the rule by hand.

## Half of it was fixed independently, and that is the interesting part

`e8a091b96 fix(#0896): the C++ RX bound is FRAMED — two artifacts that missed
their parent's change` landed on `main` while this was being written. It
regenerated the three goldens (136 / 160 / 136) and pointed the main assertion
at `transport_framed`. That half is not this issue's work and is not repeated
here.

**It fixed one of the two hand-derivations.** `rust_nested_expectation`, forty
lines below the one that was corrected, still read:

```rust
match (x1, x2) {
    (Some(a), Some(b)) => Some(a.max(b)),
    _ => None,
}
```

and it was **passing**, which is exactly why it was missed. Its type is
`Inner{i64,f64} + i32` — already 4-aligned, so the framing is a no-op for that
one type and the bare max agrees with the emitter by coincidence. A second
spelling that is right by accident is still a second spelling: it stops
agreeing the moment the corpus gains a field of odd width, and it then fails as
a mismatched NUMBER rather than as the rule it actually encodes.

Fixed here: it calls `transport_framed` too.

## Why nobody noticed for a day

`codegen_golden` runs in `check-cli-tests`, which is on `check::build`, and per
CLAUDE.md `check-build` is `schedule` / `workflow_dispatch` only — it was taken
off the merge group because it could never pass there. **No pull request and no
merge group runs it.** The required `CI` context is `check-fast` + `test-unit`,
both green on the commit that broke it.

Meanwhile `check::build` stopped the local tier at step 3 of 6, withdrawing
`check::api-parity`, `test-unit` and `test-lane-contracts` from everyone running
`just ci gate` for a day — issue 0952's point, arriving.

So the two tests that pin the derived bound get their own FAST-line gate,
`just check codegen-size-bound-golden`. Not `cli-tests` wholesale: that runs the
whole `packages/cli` workspace including a plan-pipeline e2e that compiles probe
crates at runtime, which is what `check-lane-contracts` forbids on a
merge-gating lane. These two need none of it — measured **8.1 s from a cold
target dir, 0.1 s warm**, inside the fast line's existing spread (slowest gate
~11.6 s).

This is the half that keeps the class from recurring: `e8a091b96` repaired the
values, and nothing yet stopped the next drift from sitting on `main` unseen for
another day.

## Acceptance

* [x] `just ci gate` reaches step 6 on a pristine `main`.
* [x] The RX rule has one derivation — BOTH sites call `transport_framed`.
* [x] A merge-gating lane runs the golden + parity tests, so the next drift is
      caught on the pull request rather than by whoever next runs the local
      tier.
