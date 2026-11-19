---
id: 2719
title: "Changelog fragments and example READMEs cite phase, issue and RFC ids"
status: open
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
