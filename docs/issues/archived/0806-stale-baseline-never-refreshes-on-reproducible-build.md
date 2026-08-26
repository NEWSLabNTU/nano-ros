---
id: 806
title: "A REPRODUCIBLE rebuild cannot clear a STALE verdict: the content probe
  uses the artifact's own hash as its `was rebuilt` signal, so a build that emits
  identical bytes leaves the baseline frozen and every source that moved before
  it permanently `changed`"
status: resolved
type: bug
area: testing
related: [issue-0445, issue-0764, issue-0196, issue-0807, phase-382]
---

## Problem

`candidates_changed_content_policy` (`fixtures/staleness.rs`) decides freshness
by comparing SOURCE CONTENT against a `.nros-srcbaseline` recorded beside the
artifact. It refreshes that baseline in exactly one place:

```rust
if stored_bin != Some(bin_hash) {      // "the binary changed, so it was rebuilt"
    write_srcbaseline(...);
    return Some(false);
}
```

The signal is the artifact's own content hash. **That question is not the one
being asked.** "Were these bytes produced by a different compilation?" and "did a
build run after those sources changed?" have different answers whenever the
build is REPRODUCIBLE — which ours is, and which is a property we want.

So: a source edit lands, the image is rebuilt, the rebuild emits byte-identical
output (the edit did not reach this image), the hash is unchanged, the baseline
is never rewritten, and the edited source compares "changed" against a baseline
that can no longer move. The verdict is **absorbing**: no number of rebuilds
clears it.

This is issue 0445's absorbing-STALE shape with a new cause, and CLAUDE.md's rule
("if the rebuild does not clear it, suspect the probe before trusting the
verdict") is exactly right — the probe was wrong.

## Evidence

phase-382 tier 2, `build-cortex-m-c-talker-zenoh`:

* **five consecutive STALE verdicts** across **two full `just zephyr
  build-fixtures` runs**;
* `find <every watched path> -newer <elf>` returned **nothing** — not the leaf,
  not `zephyr/`, not `packages/core`, not `packages/rmw/zenoh`;
* the `.nros-srcbaseline` mtime stayed at **09:57** while the elf was rebuilt at
  **10:54** and **12:02**. Since `write_srcbaseline` runs on ANY hash change, an
  untouched baseline is proof the rebuild was byte-identical.

The verdict also prints `probe: examined 0 input(s)`, which reads as damning and
is a red herring — half 2 does not feed that accounting. The baseline mtime is
the real evidence.

Cost: it silently disabled a tier-2 runtime cell. Compounded by issue **0807**,
which relabelled the STALE as "not prebuilt" and filtered it out of the failure
count, so nothing reported the cell had stopped running.

## Fix (2026-08-26)

**An artifact NEWER THAN EVERY INPUT is fresh by definition** — the build ran
after the last source touch, whatever bytes it produced. That check now runs
FIRST, before any baseline comparison, and refreshes the baseline when it
passes.

The content machinery keeps its job, which was always the opposite case: when a
source genuinely looks newer, tell a real edit from a git-induced mtime bump
(issue 0764's treadmill). It is no longer asked to answer a question it cannot.

Also: `write_srcbaseline` now records the artifact's mtime (`bin <hash>
<mtime_nanos>`) so "was rebuilt" has a signal that moves even when the bytes do
not. Pre-0806 baselines parse without it and keep the old behaviour, so nothing
goes stale at once on upgrade.

**Verified in all three directions**, which matters because a probe that
forgives everything turns museum binaries into silent passes (issue 0196):

| case | before | after |
| --- | --- | --- |
| reproducible rebuild, nothing newer | STALE (5×, absorbing) | **fresh** — cell runs, 3.7 s |
| real content edit to `Talker.c` | STALE | **STALE** |
| that edit reverted (mtime newer, bytes identical) | — | **fresh** |
