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
them. `check-user-surface-ids` reads both, but for internal project names
only (`name_only_hits`, `NAME_ONLY_SCOPE`). Their ids are not checked.

Counted on 2026-11-19 with the check's own `ids()`:

| Scope | Files read | Ids | Files with an id |
|---|---|---|---|
| `changelog.d/*.md` | 646 | 408 | 346 (both scopes together: ~450) |
| `examples/**/README*` | 108 | 468 | 107 (see the row above) |

## A second surface: the links towncrier adds

`pyproject.toml`'s `[tool.towncrier]` sets
`issue_format = "[#{issue}](https://github.com/NEWSLabNTU/nano-ros/issues/{issue})"`.
A fragment named `NNNN.<type>.md` is therefore rendered with a `[#NNNN]`
link, whatever its text says. That id is in the file name, not the text, so
no text check sees it. Worse, `NNNN` is the `docs/issues/` number, which is
not the GitHub issue of that number, so the link leads to an unrelated
GitHub issue. Fragments named `+<slug>.<type>.md` carry no link.

## Plan

1. A per-file ratchet for the two scopes, like the API-doc one
   (`.config/api-doc-ids-ratchet.json`, `--update-ratchet`), so no new id
   lands from the first commit.
2. Decide the towncrier link: drop `issue_format` and name fragments
   `+<slug>`, or map to something a user can open. Then make
   `changelog-add` name new fragments to match.
3. Sweep both scopes down to zero. A fragment describes a change, so
   replace an id with what changed and why; do not just delete it. A
   README replaces an id with the fact it stood for, or moves contributor
   material out of the README.
