---
id: 445
title: A staleness verdict is terminal and self-explaining, so it absorbs whatever the fixture would have done at runtime
status: open
type: bug
area: testing
related: [issue-0442, issue-0444, issue-0350, issue-0196, issue-0222]
---

## The defect

When a freshness probe calls a fixture stale, the fixture is never launched. The
cell's verdict becomes "STALE", the remedy prints itself ("Run
`just build-test-fixtures` first"), and the runtime result that would have
happened is not merely unknown — it is *replaced* by an explanation that reads
as complete.

That makes a staleness verdict absorbing: it hides a runtime defect for as long
as it persists, and it tells the reader not to look.

## Demonstrated, not hypothetical

Concrete chain, 2026-08-06:

1. Issue 0442 — the cmake freshness probe applied its
   `REGENERATED_INPLACE_HEADERS` exemption on one arm and not its sibling, so
   every freertos / threadx-linux C and C++ zenoh fixture read stale against
   `zpico-sys/c/include/zpico.h` (a cbindgen header written in place; mtime
   moves, content does not).
2. Those cells therefore never ran.
3. Fixing 0442 made them run. Seven recovered immediately. The eighth,
   `Freertos::Rust`, **failed at runtime** — boots, brings up LAN9118 + lwIP,
   never reaches "Application setup complete". Filed as issue 0444.

0444 had been sitting behind 0442. Nobody could have found it while the cell was
reported stale, and the report gave no reason to suspect anything else was
wrong.

I hit the reader-side half of this too: asked why those cells were stale, I first
answered "this branch changes a core crate, so main-built fixtures are stale."
Plausible, consistent with every observable, and wrong. The verdict is
self-explaining enough that a *wrong* explanation of it survives scrutiny.

## Why the existing rules do not cover it

* Issue 0350 — "a coordinate that never runs looks the same as one that cannot"
  — is the closest, and it is about SKIPS specifically. This is broader: in
  `rtos_e2e` a stale fixture PANICS (`build_pair` → `panic!`), a hard failure,
  and it still masks, because the failure is attributed to staleness rather than
  investigated.
* Issue 0196 — a guard narrower than its rule — explains how 0442 happened, not
  what 0442 then concealed.

The gap is the ABSORBING property itself, which is independent of whether the
verdict is a skip or a failure.

## Directions

None of these is obviously right; the tradeoffs are real.

1. **Make false-stale loud rather than terminal.** A probe cannot know it is
   wrong, but it can report *what* it compared. Printing the exemption decisions
   ("skipped N regenerated headers; tripped on X") would have shown `zpico.h`
   was being examined by one arm and exempted by another.
2. **Run anyway, and label.** Launch the fixture, report the runtime result, and
   mark the cell "result from a possibly-stale binary". Turns a hidden defect
   into a caveated observation. Risks the museum-binary problem the probes exist
   to prevent — CLAUDE.md's "long-unrebuilt families pass on museum binaries" —
   so it would need the label to be impossible to ignore in a summary.
3. **Track how long a coordinate has been non-running.** A cell stale for N
   consecutive runs is a different signal from one stale since the last edit.
   Cheap to record, and it converts "never ran" from invisible to countable.

Direction 3 is the smallest and composes with either of the others: the point is
that "this cell has not produced a runtime result in a while" should itself be
reportable, because that is the state in which defects accumulate unseen.

## Scope note

Not a request to weaken the probes. 0442 was a probe being WRONG; 0433 was a
probe being RIGHT about a genuinely shared artifact. Both masked. The property
under discussion is what a staleness verdict does to everything downstream of
it, regardless of whether the verdict is correct.
