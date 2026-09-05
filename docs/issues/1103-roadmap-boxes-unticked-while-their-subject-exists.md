---
id: 1103
title: "A roadmap box can stay UNTICKED while the file, function or recipe it
  names is in the tree — `check-roadmap-claims` never asks"
status: open
type: bug
area: tooling, docs
related: [1071, 1085]
---

## What was found

Auditing the eleven oldest phase docs still in `docs/roadmap/` (active), against
a tree whose newest phase is 429: **phase-215 carried seven boxes that had
landed and were never ticked.** Each is a claim about something that either
exists in the tree or does not:

| box | claim | in the tree |
| --- | --- | --- |
| 215.A.1 | define the `board.cmake` schema, nine `NROS_BOARD_*` variables | all nine present in `docs/reference/board-cmake-schema.md` AND in the fvp-aemv8r-smp `board.cmake` |
| 215.A.3 | schema doc cross-references this phase | `board-cmake-schema.md:15` |
| 215.B.2 | `zephyr/CMakeLists.txt` includes the new fn | `zephyr/CMakeLists.txt:53,60`, comment cites "Phase 215.B.2" |
| 215.B.3 | callable BEFORE `find_package(Zephyr)` | `nano_ros_use_board.cmake:8` usage + `:27,35` enforcement |
| 215.D.4 | recipes delegate to `west fvp run` | `scripts/west_commands/fvp.py`, `scripts/west-commands.yml`, `just/zephyr-setup.just:239` |
| 215.I.1 | `book/src/porting/board-crate-import.md` | exists |
| 215.I.2 | SUMMARY.md update | `book/src/SUMMARY.md:100` |

The phase read as 20 of 38 with a `**Status.** OPEN` line implying the schema
work had not started. It was 27 of 38, and the in-repo half was complete.

## Why nothing caught it

`check-roadmap-claims` is the gate for this file and it PASSES — *"0 known
finding(s), no new ones across 68 active phase doc(s)"*. Its rules ask:

* R1/R2 — does the status TEXT agree with the box COUNTS?
* R3 — does an `**Owns:**` line name an issue that is `status: resolved`
  without saying so?
* R4 — is a document in `docs/roadmap/` disclaiming that it is a phase?

Every one compares the document against ITSELF or against an issue's
frontmatter. **None asks whether a box's SUBJECT is already in the tree.** So
"unticked but done" is structurally invisible: the status line and the counts
agreed with each other, and both were wrong about the world.

This is 1071's shape a third time — the gate exists, runs, is green, and the
rule it enforces is narrower than the rule its name implies. 1071 was a registry
whose count could silently fall; 1085 was a link whose target could silently
move; this is a checkbox whose subject can silently arrive.

## What is checkable, and what is not

Deliberately narrow, because the failure mode of a clever version is a gate
that nags about prose:

**CHECKABLE — a box that names a concrete artifact.** The seven above are all
of one shape: a backticked path (`book/src/porting/board-crate-import.md`,
`scripts/west_commands/fvp.py`), a cmake function (`nano_ros_use_board()`), or
a recipe. If an UNTICKED box names a path that EXISTS, that is worth one line of
output — not an error, a report, because the box may name a file that exists for
another reason.

**NOT CHECKABLE.** "Parity check vs Phase 175.A native cyclonedds Rust
talker/listener: byte-equal wire format" (217.C.3) names no artifact and blocks
on a real FVP run. Neither does "ASI's `actuation_module/CMakeLists.txt` builds
clean against the Zephyr 3.7 floor" — an external repo this worktree cannot see.
A gate that reported those would be reporting on documents it cannot read.

**A WARNING, not a failure**, and that distinction is the design. A ticked box
is a human's verdict that the work is DONE, which is more than "a file with this
name exists"; the gate can say "this looks landed, check it" and cannot say "tick
it". Making it fail would push people to tick boxes to get green, which is worse
than the drift.

## Where it bites

The same class the session that found it hit six times in issues: work lands,
the record does not move, and the next person re-derives or re-does it. For a
roadmap the cost is specific — phase-215 is `**Priority.** P1`, so anyone
scanning for what to pick up read seven finished items as available work.

## Proposed

`check-roadmap-boxes`, on the fast line: for every UNTICKED box in an active
phase doc, extract backticked tokens that look like a repo-relative path, and
report the ones that resolve against `git ls-files`. Index lookup, no walk
(issue 0844). Ratcheted like `check-gate-selftests` so the known set may only
shrink — the phases audited here are the seed, and a new occurrence is the
signal.

Not done in the same change as the phase-215 fix, deliberately: the fix is a
verified edit to one document, and the gate is a new rule over 68 of them.
