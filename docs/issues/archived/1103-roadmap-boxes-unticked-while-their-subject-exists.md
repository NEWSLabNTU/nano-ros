---
id: 1103
title: "A roadmap box can stay UNTICKED while the file, function or recipe it
  names is in the tree — `check-roadmap-claims` never asks"
status: resolved
resolved: 2026-09-06
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

## FIXED 2026-09-06 — and the rule I proposed was the wrong one

`check-roadmap-boxes`, fast line, ratcheted. It does NOT do what this issue
proposed, because the proposal was measured against the known answer and failed
it.

### The per-box rule catches 2 of the 7

"For every UNTICKED box, extract backticked tokens that look like a path, report
the ones that resolve." Run against the pre-fix phase-215:

    caught  215.B.2 (`zephyr/CMakeLists.txt`), 215.I.1 (the book page)
    missed  215.A.1  names cmake VARIABLES, not a path
            215.A.3  "Documented schema cross-references this phase doc" — prose
            215.B.3  names a FUNCTION and a call-order constraint
            215.D.4  names `just zephyr run-fvp-aemv8r*`, a recipe that does not exist
            215.I.2  "SUMMARY.md update" — no `/`, so not a path

And on today's tree it fires on **2 boxes, both false positives**: phase-216's
216.A.6 and phase-325's W3.4 each name a file they intend to EDIT, which of
course exists. 29 % recall on the motivating case and 0 % precision on main is
not a gate; it is a nuisance that gets switched off.

### What works is one level up

A phase SECTION carries a `**Files:**` line naming what it creates. When every
path that line declares is in the index and the section still has open boxes,
the work described has arrived and the record has not moved.

Measured on the same pre-fix document: **3 of the 4 relevant sections** — 215.B,
215.D, 215.I, five of the seven boxes — with the evidence printed:

```
  ### 215.D — `west fvp` extension (moves Phase 214.A runner)
      1 open box(es); all 4 declared file(s) exist:
        scripts/west_commands/fvp.py
        scripts/west-commands.yml
        zephyr/module.yml
        just/zephyr.just
```

Zero findings on current `main`, so the baseline starts empty.

**The fourth section is missed for a reason worth keeping.** 215.A's
`**Files:**` line names
`packages/boards/nros-board-fvp-aemv8r-smp/board.cmake`; the file is at
`packages/boards/nros-board-zephyr/boards/fvp-aemv8r-smp/board.cmake`. The doc's
own path is stale, so the section reads 1-of-2 and is not flagged — the gate
declining to guess, which is the behaviour to keep. (That stale path is a second,
smaller finding this exercise turned up.)

### And it FAILS, where this issue argued for warn-only

I argued a failing gate would push people to tick boxes for green. That is right
about a per-BOX rule and wrong here, and `check-gate-selftests`' own argument
settles it: a warning nobody must act on decays into a comment.

The ratchet is the resolution. The known set is committed and may only SHRINK; a
new entry fails with a one-line escape. Ticking a box to silence it is not
available, because **the flag is on the SECTION, not the box** — the ways out are
to finish the section, to correct a `**Files:**` line that names a path that
moved, or to baseline it with a reason.

Self-test on the normal path, 5 cases. Mutation-checked against the real
pre-fix `phase-215`: 3 findings before, 0 after.
