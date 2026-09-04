---
id: 1053
title: "RFC ids have no reservation, so two sessions both wrote 0087 — the same race `just issue-new` and `just phase-new` exist to stop"
status: open
type: bug
area: docs, tooling
severity: medium
found: 2026-09-04
related: [0884, phase-395, phase-420, phase-421]
---

# Two RFCs numbered 0087, hours apart, both correct by the documented rule

`docs/design/README.md` says how to start an RFC:

> **New RFC:** copy [0000-template.md](0000-template.md) to `NNNN-slug.md`, next
> free number.

"Next free number" is read-then-write. Two sessions did exactly that on
2026-09-04:

- PR #329 (opened 03:29Z) adds
  `docs/design/0087-ros2-api-adoption-and-the-compile-or-conform-rule.md`.
- PR #367 (merged 10:47Z) added
  `docs/design/0087-package-identity-and-provider-format.md`, plus 0088.

Neither author did anything wrong. `main` now holds one 0087 and an open PR
carries a different one, so that branch will conflict on
`docs/design/README.md` and land a duplicate number if merged as-is.

## The tree already solved this twice, for the other two series

The identical race, for issues, has collided **seven times** — 0367 → 0372 →
0377 collided *twice*, the second time while renumbering the first. The remedy
is a reservation, not care:

| Series | Reservation | Enforcement |
| --- | --- | --- |
| issue | `just issue-new <slug>` claims `refs/issue-ids/NNNN` on origin | `pre-push` hook refuses a duplicate even if the tool was skipped |
| phase | `just phase-new <slug>` claims `refs/phase-ids/NNNN` | — |
| **RFC** | **nothing** | **nothing** |

`scripts/reserve-claim.sh` is already general: an id is any
`[A-Za-z0-9._-]` string, so `scripts/reserve-claim.sh claim rfc-0087` works
today and is what surfaced this collision — the claim succeeded, which proved
nobody else had claimed it, and the file on `main` proved the number was taken
anyway. **A claim is not a reservation**: claims expire and are released, and
this one was released on completion, so the ref cannot be the record.

## Why the ledger fix (0884) does not cover it

Issue 0884 made `docs/issues/open.md` a generated file with `merge=union`, so
concurrent filings stop conflicting. That solves the *index* conflict, not the
*identity* collision: two RFCs numbered 0087 are still two RFCs numbered 0087,
and a `merge=union` README would simply carry both rows.

## Fix

Add `just rfc-new <slug>`, mirroring `just phase-new`:

1. claim `refs/rfc-ids/NNNN` on origin, taking the next number the remote does
   not already hold — so the allocation is atomic against other sessions rather
   than against the local checkout;
2. copy `0000-template.md`, fill `rfc:` and the slug;
3. print the README row to add.

Then extend the `pre-push` hook the way it already refuses a duplicate issue id:
a pushed `docs/design/NNNN-*.md` whose number exists on `origin/main` under a
different slug is a duplicate, and the hook can see that without network beyond
the fetch it already does.

Amend `docs/design/README.md` so "next free number" stops being the instruction.

## Cost of not fixing it

A renumber. Cheap while the loser is an open PR, as here; expensive once both
have merged and the number is cited from phase docs, issue `related:` lists and
commit messages — which is the state issue 0884's renumbering commits describe
("docs: renumber — phase 341 -> 345, issues 0467-0471 -> 0492-0496", and a
second one for the same collision a day later).
