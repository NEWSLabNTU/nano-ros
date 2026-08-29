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

## Dev utilities: check the environment, then install into it

The line is drawn around the INTERPRETER, not around writing:

* nano-ros does not provision an interpreter — no venv creation, no choosing
  between system / `--user` / pipx on your behalf. That is a decision about your
  machine, and a wrong guess surfaces four frames inside cmake as `Error finding
  board: mps2`.
* It does install the repo's OWN tools into the interpreter you already chose.
  `towncrier`, or a pinned `clang-format`, is not an environment decision — it
  is a tool the repo needs in order to work.

        just dev-tools              report what is missing
        just dev-tools --install    install it into the probed interpreter

DEV groups only: `--install zephyr-build` is refused. A build environment is the
user's to assemble, and installing into it silently is how three interpreters
end up in play with nobody knowing which one a lane will use.

On a PEP 668 host the system interpreter refuses and pip says so — better than
any pre-flight guess, so the script lets pip speak and translates the remedy
(venv, or `--user`).

## Verified end to end, on a real install

* `just dev-tools` reported towncrier missing and printed the install line.
* `--install` exercised in a CLEAN VENV, the full cycle: both packages MISSING,
  installed, RE-PROBED, `OK — dev-tools now satisfied`. The re-probe is not
  ceremony — a pip that exits 0 has been seen to leave an import failing when a
  second interpreter shadows the first, so success is asserted by importing,
  not by pip's return code.
* `--install zephyr-build` refused, with the installable set named.
* `just changelog-add` wrote `0885.feat.md` and `0884.fix.md`.
* `just changelog` previewed both under `Fixed` / `Added` with issue links.
* `just changelog-release 9.9.9-test` consumed both fragments into
  `CHANGELOG.md` and left `changelog.d/README.md` untouched — confirming the
  README is not mistaken for a fragment. Reverted afterwards; no fake release
  is committed.

## A second bug my own test caught

The refusal for non-dev groups was written INSIDE the "something is missing"
branch, so `--install zephyr-build` printed OK on a host that happened to have
those packages and refused on one that did not. A permission question must not
have two answers. Moved before any probing.

## One trap found by running it, and documented in the recipe

The fragment text reaches a bash recipe, so BACKTICKS ARE COMMAND SUBSTITUTION.
The first fragment written came out as "towncrier fragments replace a shared
CHANGELOG;  writes one" — the backticked words evaluated and gone. Markdown code
spans are exactly what a changelog line wants, so this will bite; the recipe now
says to quote the text and prefer single quotes when it contains backticks.
