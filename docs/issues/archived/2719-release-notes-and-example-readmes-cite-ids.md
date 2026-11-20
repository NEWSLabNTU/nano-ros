---
id: 2719
title: "Changelog fragments and example READMEs cite phase, issue and RFC ids"
status: resolved
resolved: 2026-11-21
type: limitation
area: [docs, release]
severity: low
found: 2026-11-19
related: [2581]
---

## What

Changelog fragments become the release page, and an example's README is
what a user copies from, so both are user text and the no-ids rule covers
them. `check-user-surface-ids` did not read either.

Counted on 2026-11-19 with the check's own `ids()`:

| Scope | Files read | Ids | Files with an id |
|---|---|---|---|
| `changelog.d/*.md` | 646 | 408 | 346 (both scopes together: ~450) |
| `examples/**/README*` | 108 | 468 | 107 (see the row above) |

## A second surface: the links towncrier added (fixed)

`pyproject.toml`'s `[tool.towncrier]` set
`issue_format = "[#{issue}](https://github.com/NEWSLabNTU/nano-ros/issues/{issue})"`,
so a fragment named `NNNN.<type>.md` rendered with a `[#NNNN]` link, whatever
its text said. `NNNN` is the `docs/issues/` number, not the GitHub issue of
that number, so the link led to an unrelated GitHub item; `CHANGELOG.md`'s
0.6.0 section held 150 of them. PR #4298 set `issue_format = ""` (no
reference rendered), stripped the 150, and added
`check-changelog-issue-links`. A fragment's number now reaches no reader, so
`changelog-add` may keep naming fragments by it.

## Plan

1. Done: `check-user-surface-ids` reads both scopes, and
   `.config/user-text-ids-ratchet.json` holds their ids per file (790 in 455
   files when it landed; link targets and HTML comments do not count),
   written by `--update-ratchet` with the API-doc one. No new id lands.
2. Done in PR #4298: the towncrier link (above).
3. Sweep both scopes down to zero. A fragment describes a change, so
   replace an id with what changed and why; do not just delete it. A
   README replaces an id with the fact it stood for, or moves contributor
   material out of the README.

## Resolution (2026-11-21)

Plan item 3 is done: the ratchet holds no file. The sweep found the gate's
reach narrower than its rule in two places, and both are closed:

- `USER_TEXT_FILE` matched `examples/.+/README`, which needs a directory below
  `examples/`, so `examples/README.md` itself (72 rendered ids) was never
  read. Its contributor material (the tree's history, the layout classes,
  how the matrix is tested, the gaps behind the empty cells) moved to the
  Internals book as `testing/example-tree-layout.md`, where its ids belong;
  what stays is copy-out text with the facts stated.
- `CHANGELOG.md`, the release history the fragments render into, was not
  read either (68 ids, two internal project names). Each id is replaced by
  the fact it stood for.

The pattern now reads `examples/README.md` and `CHANGELOG.md`, and the
self-test asserts both are in scope, so a narrower pattern fails it. The
per-release pages under `book/src/migrating/releases/` are user-book pages,
already read by the book scope, and held no id.

