---
id: 885
title: "Adopt towncrier for release notes, and declare the dev utilities that
  were only ever literals in a Dockerfile"
status: resolved
type: tech-debt
area: build, docs
related: [issue-0884, phase-395, phase-396]
resolved_in: "issue 0885"
---

## Two gaps, one shape

**No changelog at all.** The repo had no `CHANGELOG.md` and no release notes.
Adding a conventional one would have imported the defect issue 0884 had just
removed from the issue ledger: a single file every pull request edits is a
shared registry, so every pull request conflicts with every other.

**No declared dev utilities.** `clang-format==17.0.5` existed only as a literal
inside the CI Dockerfile. A contributor's host version reformats the tree
differently and nothing says so. A version that matters belongs somewhere
`--list` shows it.

## Adopted: towncrier

Upstream's argument is the one this repo already reached independently — a
monolithic changelog is "prone to merge conflicts", so contributors write
independent FRAGMENTS and the file is assembled at release. The issue ledger is
already one-file-per-issue, so the fragment half of the pattern was already
right here; only the assembled artifact was wrong.

    changelog.d/<issue>.<type>.md     one file per change, never a shared region
    just changelog-add 885 feat "…"   write one
    just changelog                    preview, fragments untouched
    just changelog-release 0.6.0      assemble into CHANGELOG.md, delete fragments

Types are `fix` `feat` `perf` `breaking` `docs` — the repo's own commit
vocabulary, not towncrier's defaults, and fragments are named after the ISSUE
they close, which is how work is already tracked here.

## Dev utilities: REPORT, do not install

`scripts/check-python-deps.py` gains a `dev-tools` group (towncrier,
clang-format). It stays report-only, and that is deliberate rather than lazy —
the script's own header explains why nano-ros does not provision Python: PEP 668
externally-managed interpreters, `--user` vs venv vs pipx, and up to three
interpreters in play at once, where a wrong guess surfaces four frames inside
cmake as `Error finding board: mps2`.

    just dev-tools

names what is missing, for the interpreter it probed, with the exact
`pip install`. `just changelog*` needs towncrier from that group.

## Verified end to end, on a real install

* `just dev-tools` reported towncrier missing and printed the install line.
* `just changelog-add` wrote `0885.feat.md` and `0884.fix.md`.
* `just changelog` previewed both under `Fixed` / `Added` with issue links.
* `just changelog-release 9.9.9-test` consumed both fragments into
  `CHANGELOG.md` and left `changelog.d/README.md` untouched — confirming the
  README is not mistaken for a fragment. Reverted afterwards; no fake release
  is committed.

## One trap found by running it, and documented in the recipe

The fragment text reaches a bash recipe, so BACKTICKS ARE COMMAND SUBSTITUTION.
The first fragment written came out as "towncrier fragments replace a shared
CHANGELOG;  writes one" — the backticked words evaluated and gone. Markdown code
spans are exactly what a changelog line wants, so this will bite; the recipe now
says to quote the text and prefer single quotes when it contains backticks.
