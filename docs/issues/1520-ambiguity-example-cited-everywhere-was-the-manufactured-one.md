---
id: 1520
title: "The `entry =` ambiguity every doc cites was the one issue 1517 measured as
  manufactured, and the tree now has no measured ambiguity case at all"
status: open
type: docs
area: [docs, examples, cli]
severity: low
found: 2026-09-27
related: [1517, 1288, 1511]
---

## What this is

`[image.*] entry` exists for the case where `nros build` cannot derive a Zephyr
image's application. Six places in the tree explained it with the same example
(all six as of 2026-09-27, before 1517 corrected them):

| where | the claim |
| --- | --- |
| `ImageBlock::entry` doc-comment | `realtime-cpp` has two, both `DEPLOY zephyr`, both on `native_sim/native/64` |
| `west_application_dir` doc-comment | same |
| `book/src/getting-started/integration-zephyr.md` | same |
| `book/src/getting-started/workspace-entry-pkg.md` | same |
| `book/src/user-guide/component-and-entry-pkg.md` | same |
| `docs/design/0085-zephyr-workspace-and-west-handoff.md` | same, plus "Six of the fourteen Zephyr images match more than one entry package" |

Issue 1517 measured that example: the ambiguity was MANUFACTURED by
`[image.fvp] board` misreading as `native_sim/native/64`. With the board
corrected the two entries no longer collide.

**1517 corrected all six sites** — both doc-comments, the three book pages, and a
dated correction note on 0085. What is left open here is the part a text edit
cannot settle, below.

## And the replacement example is not one either

The obvious substitute is the other case 0085 names — `examples/workspaces/rust`
with `zephyr_entry` and `zephyr_entry_robot1`, both Rust entries on
`native_sim/native/64`, where 0085 says a first-match scan "would have built
`zephyr_entry`" for `[image.zephyr_robot1]`. Measured 2026-09-27 (`nros sync`
then `nros build <id> --dry-run`, one `entry =` dropped at a time):

* `[image.zephyr]` with no `entry` → derives `src/zephyr_entry`, **correctly**.
  Its `entry =` is redundant.
* `[image.zephyr_robot1]` with no `entry` → **0 candidates**, falls through to
  the bringup directory, dies on `conf fragment prj-zenoh.conf not found`.

Neither is an ambiguity. The reason is in `west_application_dir`: a RUST entry
matches on `l.image == image_id` — the entry's own `system.toml` names the image
it serves — so a Rust entry claimed by one image is never a candidate for a
sibling image on the same board. The ambiguity arm is reachable only for C/C++
entries, which match by `DEPLOY` token.

So after 1517 the tree has **no measured in-tree example of the ambiguity arm**,
and the `entry =` rows split into "required for the 0-candidate reason" and
"redundant". Which is which is a measurement nobody has taken across the 10
rows.

## Why it matters more than a stale example

Both failure arms print different messages and want different fixes, and the
docs describe only one of them. 1517 spent its first pass reasoning from the
comment, predicted that correcting the board would make the derivation work, and
was wrong — the derivation went from 2 candidates to 0. A reader who trusts these
six sites will make the same prediction.

## Acceptance

* ~~The six sites describe BOTH arms and cite a measured example, or none.~~
  Done in 1517.
* 0085's count restated from a measurement rather than left under a correction
  note.
* The 10 `entry =` rows in `examples/**/system.toml` classified by dropping each
  and reading the result: redundant ones removed, required ones commented with
  which arm they answer. Two of the ten are measured so far —
  `examples/workspaces/rust` `[image.zephyr]` is REDUNDANT and
  `[image.zephyr_robot1]` is REQUIRED (0-candidate arm) — and the reason those
  two differ is not yet understood, since both are Rust entries naming their own
  image. That is the question to answer first; the other eight follow from it.
* Issue 1288 makes the whole question moot for generated entries; this is worth
  only the gap before it lands, and the prose correction is worth it regardless.
