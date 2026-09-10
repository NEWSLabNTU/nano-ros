---
id: 1185
title: "`the_static_is_handed_out_once_and_only_once` fails IN SUITE and passes
  solo — the latch it asserts on is process-global and another test takes it first"
status: resolved
resolved_in: "605d667eb"
type: bug
area: [core, testing]
related: [1171, phase-392]
---

## What

`cargo test -p nros-node --lib --features std,sim-time,param-services` — the
first line of `check-build`'s `node-std-tests` lane — fails:

```
executor::backing::tests::the_static_is_handed_out_once_and_only_once --- FAILED
thread '…' panicked at packages/core/nros-node/src/executor/backing.rs:232:9:
an over-large request must not claim the latch
test result: FAILED. 365 passed; 1 failed; 1 ignored
```

Run the same test ALONE and it passes:

```
$ cargo test -p nros-node --lib --features std,sim-time,param-services --quiet -- \
    --exact executor::backing::tests::the_static_is_handed_out_once_and_only_once
test result: ok. 1 passed; 0 failed; 366 filtered out
```

## Why

`TAKEN` is a process-global `AtomicBool`, and the test says so in its own
doc-comment — it merges the positive control and the two `None` cases into ONE
test precisely because a second test would observe an already-consumed latch.
But the reasoning stops at the module boundary: **any other test in the same
binary that reaches `Executor`'s `alloc` convenience constructor also calls
`backing::take`**, and `cargo test` runs 367 tests on a thread pool in one
process. Whether the backing test wins the race is scheduling, not code.

So the assertion that fires is the WEAKEST one — `!is_taken()` before the
handout — and its message ("an over-large request must not claim the latch")
describes a defect that is not there.

## Not caused by the branch that found it

Reproduced on `origin/main` at `d2377affd` with no local changes: same test,
same line, same message. It was found while landing issue 1171, whose only
`nros-node` edits are comments in `build.rs` and a module doc.

## Why nobody has seen it

`node-std-tests` runs in **`check-build`**, which phase-396 W1 moved to
`schedule` / `workflow_dispatch` only — it gates no merge. It is also not in
`just ci gate`'s cheap half, so a developer meets it only when running the full
local tier, and then it reads as their own change.

## What to do

Not "add `--test-threads=1`" — that hides the coupling and costs the whole
binary's parallelism for one test. The shapes worth considering:

* Give the latch test its own PROCESS, the way `node-std-tests` already gives
  `boot_report::tests` two cargo invocations for exactly this reason (the
  comment there spells the argument out) — the precedent is in the same recipe.
* Or make the assertion order-independent: capture `is_taken()` before the
  over-large request and assert it did not CHANGE, rather than that it is false.
  That still proves what the case is for ("an over-large request is refused
  before the latch is touched") and stops depending on being first.

The positive control (`take` actually succeeds once) genuinely needs a fresh
process, so it cannot be made order-independent — which is the argument for the
first shape.

## RESOLVED 2026-09-11 — fixed by `605d667eb`; one defect, three ids

`605d667eb fix(#1183, #1186): the backing latch test needs a virgin process`
(2026-09-06) marks `the_static_is_handed_out_once_and_only_once` `#[ignore]` and
runs it alone, in its own process, from the lane:
`just/check/lanes.just:1709` —
`cargo test … -- --ignored --exact executor::backing::tests::the_static_is_handed_out_once_and_only_once`.
The latch is process-global, and in a separate process no sibling test can take
it first.

Checked before archiving, because an ignored test is dead coverage until the
lane that is meant to run it is shown to run it:

* the test still exists — `packages/core/nros-node/src/executor/backing.rs:240`;
* `just check node-std-tests` passes, and its dedicated invocation reports
  `1 passed; 0 failed; … 447 filtered out` — exactly that one test.

Issues 1183, 1185 and 1186 are the same defect filed three times. The fix commit
named 1183 and 1186 and missed 1185, and it archived none of them, so all three
stayed open for five days after the fix landed.
