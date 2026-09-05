---
id: 1086
title: "RFC numbers are claimed by reading the highest one, which is the race `just issue-new` and `just phase-new` exist to prevent — and it collided"
status: open
type: bug
area: tooling, docs
severity: medium
found: 2026-09-05
related: [0883, 0884, phase-429]
---

# Two sessions opened RFC-0089 on the same day

`docs/design/README.md` says:

> **New RFC:** copy [0000-template.md](0000-template.md) to `NNNN-slug.md`,
> next free number.

"Next free number" is read-then-write. Two sessions that read before either
writes get the same answer, and on 2026-09-05 two did:

| file | branch | created |
| --- | --- | --- |
| `0089-ros2-api-adoption-and-the-compile-or-conform-rule.md` | `origin/phase-417-ros2-api-adoption` | 13:21 |
| `0089-codegen-version-is-the-compatibility-token.md` | phase-429 work, local | later the same day |

Resolved by renumbering the second to 0090 — but only because the collision was
noticed. Nothing detects it.

## Why this is the known race, in the one series that never got the fix

CLAUDE.md already records this exact failure for the other two numbered series:

> **reserve issue ids with `just issue-new <slug>`, never by reading the highest
> number.** Reading-then-writing is a race that has collided seven times
> (0367→0372→0377 collided TWICE, the second time while renumbering the first).

and

> Same race, same fix, third series: `just phase-new <slug>` for work needing its
> OWN phase number (two sessions opened `phase-350` for unrelated work).

Both reserve a ref on origin (`refs/issue-ids/NNNN`, `refs/phase-ids/NNNN`),
which git rejects if it already exists, and both are backed by a `pre-push` hook
that refuses a duplicate even when the tool was skipped.

**RFCs have neither.** There is `just issue-new` and `just phase-new`; there is
no `just rfc-new`, no `refs/rfc-ids/*`, and no gate that a design number is
unique. The series with the longest-lived documents is the one with no
protection.

## Why it is worse here than for an issue

An issue id appears in frontmatter and in cross-links. An RFC number appears in
**every document that cites it**, in `implements-tracked-by` frontmatter, in
phase docs, in source comments, and in `docs/design/README.md`'s table. The
renumber above touched ten files across `docs/`, `packages/`, `scripts/` and
`.github/`. Renumbering after the second one has been cited is materially harder
than renumbering an issue, and the cost grows with every citation.

And the failure is quiet. Two files named `0089-*.md` coexist happily until both
branches merge, at which point `docs/design/` has two RFC-0089s and every
`RFC-0089` citation in the tree is ambiguous with no error anywhere.

## Fix direction

1. `just rfc-new <slug>` claiming `refs/rfc-ids/NNNN` on origin, mirroring
   `just issue-new` / `just phase-new` exactly — same tool shape, same failure
   message, no third idiom.
2. The `pre-push` hook refuses a duplicate RFC number, as it already does for
   issue ids.
3. A gate asserting one file per number in `docs/design/` (including
   `archived/`), and that each file's `rfc:` frontmatter matches its filename.
   The gate is cheap and buildless; it is the part that catches a number claimed
   without the tool.

## Not the fix

**Renumbering on merge.** Whoever merges second would have to rewrite every
citation in their branch, which is the cost above paid at the worst moment.
Reservation moves the cost to the cheapest one.
