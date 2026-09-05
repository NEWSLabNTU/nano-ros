---
id: 1053
title: "RFC ids have no reservation, so two sessions both wrote 0087 — then two more wrote 0089. The same race `just issue-new` and `just phase-new` exist to stop"
status: open
type: bug
area: docs, tooling
severity: medium
found: 2026-09-04
related: [0884, phase-395, phase-420, phase-421, phase-429]
---

# Two RFCs numbered 0087, hours apart, both correct by the documented rule

`docs/design/README.md` says how to start an RFC:

> **New RFC:** copy `docs/design/0000-template.md` to `NNNN-slug.md`, next
> free number.

"Next free number" is read-then-write. Two sessions did exactly that on
2026-09-04:

- PR #329 (opened 03:29Z) adds an RFC on ROS 2 API adoption, numbered 0087.
  (It has since been renumbered to 0089 on `phase-417-ros2-api-adoption` — and
  then collided AGAIN at 0089, which is the recurrence recorded at the end of
  this issue. Not linked here: the file is on that branch and not on `main`, so
  a link would be a dangling reference of exactly the kind
  `check-doc-refs` exists to stop.)
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

## IT RECURRED 2026-09-05, on 0089 (was filed separately as #1086)

Not a hypothetical repeat: the same race, on a different number, the day after.

| file | branch | created |
| --- | --- | --- |
| `0089-ros2-api-adoption-and-the-compile-or-conform-rule.md` | `origin/phase-417-ros2-api-adoption` | 13:21 |
| `0089-codegen-version-is-the-compatibility-token.md` | phase-429 work, local | later that day |

Resolved by renumbering the second to 0090 — and **only because the collision
happened to be noticed**. Nothing detects it. The second author had read
`docs/design/README.md`, followed "next free number", and been wrong through no
fault of their own, which is this issue's whole point arriving twice.

Worth recording that the author of the losing RFC also filed this same issue a
second time, as #1086, without finding 1053 first. Reading-then-writing produced
a duplicate ISSUE about reading-then-writing producing duplicate RFCs. #1086 is
retired into this one.

### What the second instance measured

**The renumber cost ten files across four directories** — `docs/design/` (the
RFC and the README row), `docs/roadmap/` (the phase doc's
`implements-tracked-by` and prose), `packages/core/` and `packages/tooling/`
(source comments citing the RFC), `.github/workflows/` and `scripts/` (comments
in CI steps and gates).

That is the asymmetry with an issue id, and it is why the fix matters more here
than for the series that already have one: an issue id lives in frontmatter and
a few cross-links, while **an RFC number is cited from everywhere it governs**.
The cost grows with every citation, so it is cheapest at reservation time and
most expensive at exactly the moment a collision is discovered.

### And it fails quietly

Two files named `0089-*.md` coexist happily on separate branches. Nothing is red
until both merge, at which point `docs/design/` holds two RFC-0089s and every
`RFC-0089` citation in the tree is ambiguous — with no gate saying so. The
uniqueness check in the Fix section above is the part that turns this from
"noticed by luck" into "noticed by CI".
