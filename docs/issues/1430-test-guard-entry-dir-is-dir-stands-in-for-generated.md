---
id: 1430
title: "Nothing gates the class issue 1411 fixed: a test guard that uses
  `is_dir()` on an entry directory as a stand-in for \"a package was generated\",
  after that directory grew a second producer"
status: open
type: tech-debt
area: [testing, build, ci]
severity: low
related: [issue-1411, phase-445, rfc-0098]
found: 2026-09-21
---

## The class

Issue 1411 was one guard:

```rust
if entry.is_dir() { /* the entry was generated */ }
```

`build/<coord>/<entry>/` used to have exactly one producer, so its existence did
mean a package had been generated. Phase-445 W4/W5 (RFC-0098 D1) gave it a
second: `nros build` writes `nros-cargo.toml` into that same directory
**unconditionally**, including on the path where model resolution has already
failed and warned. From that commit on, the directory exists either way and the
guard tests nothing — the test took its degraded branch, asserted nothing it had
not already asserted, and reported `ok`.

The defect is not the `is_dir()` call. It is that **a guard standing in for "the
artifact was produced" keeps reading as correct after the thing it names grows a
producer**, and nothing in the tree notices. 1411's own corroboration makes the
point: three tests further down the same file,
`a_hand_written_entry_suppresses_generation` already asserted on
`…/native_entry/Cargo.toml` and carried a comment about the settings file
landing there either way. It was updated when the file moved. The guard above it
was not.

## What 1411 already did

* fixed the one guard (it now names `Cargo.toml`, the artifact the test
  `unwrap()`s);
* swept `is_dir()` / `exists()` across the CLI test targets and read every
  control-flow guard individually — one defect, and the four other entry-dir
  sites in the same file already named a file;
* found and fixed a worse sibling in `entry_typed_plan.rs` (a bare
  `eprintln!` + `return` that reported PASS with zero assertions);
* gave the six resolver preconditions one spelling.

So the instances are closed. What is open is that the next one is invisible.

## Why the general rule is not checkable

"A guard must test the artifact the test is about" needs to know which artifact
a test is about, which is not statically available. Do NOT try to gate that — a
gate that guesses is the reporting-OK-over-a-violation shape (issue 1396 wrote
two such rules and measured both wrong).

## The narrow rule that is checkable

A path under `build/<coord>/<entry>/` used as an `is_dir()` **condition** — as
opposed to an assertion about existence, which is a legitimate property under
test — in a test target. That is the exact shape that broke, it is decidable
from the source, and the fix is always the same: name the file.

Open questions for whoever takes it:

* Should the rule cover `.exists()` on a directory path too? 1411's sweep found
  those were all assertions or directory-walk filters, so the rule may be
  narrower than the grep.
* Is the right home a new gate, or a case inside `check-no-vacuous-tests`? That
  gate already owns "a test whose only effects are prints"; this is the
  neighbouring "a test whose guard cannot fail". One spelling is preferable to a
  second gate if the shapes fit (issue 0196 / the repo's one-helper rule).
* The vacuity guard matters: zero findings on a tree that has the fixed guard is
  correct, so the self-test must plant the pre-1411 shape rather than assert a
  nonzero count.

## Acceptance

A gate (or gate case) that FAILS against the pre-1411 `build_verb_pipeline.rs`
guard and PASSES against the current tree, with the planted shape in its
self-test, and a reach covering every test target rather than the one file that
had the defect.
