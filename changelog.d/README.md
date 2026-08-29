# Changelog fragments

One file per change: `<issue>.<type>.md` — e.g. `0885.feat.md`, `0870.fix.md`.
Types: `fix` `feat` `perf` `breaking` `docs` (see `[tool.towncrier]` in
`pyproject.toml`).

**Why fragments instead of editing `CHANGELOG.md`.** A single changelog file is
a shared registry: every pull request edits the same region, so every pull
request conflicts with every other one. That is the same defect issue 0884 fixed
for the issue ledger, and towncrier exists for exactly this — upstream's words
are that a monolithic changelog is "prone to merge conflicts", so contributors
write independent fragments and the file is assembled at release.

Write the fragment for a READER OF THE RELEASE, not for a reviewer of the diff:
what changed for someone using nano-ros. The issue file carries the
investigation; this carries the outcome.

    just changelog-add 885 feat "just issues queries the ledger offline"
    just changelog            # preview the assembled notes
    just changelog-release 0.6.0   # consume fragments into CHANGELOG.md

Fragments are DELETED by `changelog-release` — that is the point: once assembled
they live in `CHANGELOG.md` and in git history.
