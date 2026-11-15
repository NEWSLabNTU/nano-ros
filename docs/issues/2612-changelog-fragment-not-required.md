---
id: 2612
title: "Nothing asks for a changelog fragment, so a user-visible change can merge without one"
status: open
type: tech-debt
area: [release, ci]
severity: low
found: 2026-11-15
related: [0885]
---

**Owner:** unassigned (the shape is a policy call for the maintainer). **Home:**
no active phase owns release tooling. The release phases (phase-431, phase-443)
are archived. The nearest active doc is phase-535, whose item 5 (`release-gate`,
release preconditions on `release-nros.yml`) is related; phase-535 is parked.

## What happens

towncrier (issue 0885) builds `CHANGELOG.md` from `changelog.d/` fragments, but
no gate, hook or workflow checks that a change carries one. The only
references to `changelog.d/` in `scripts/`, `just/` and `.github/` are
exclusions in two retired-name checks plus `book-move.py` and
`stage-sdk-root.sh`. `check fast`, `ci gate` and the PR workflow never ask.

Found by the release audit for 0.6.0-nros11. #3883 changed what `ros2 node list`
shows for an image and added a C registration refusal, then merged with no
fragment. The audit caught it by hand, and the fragment was added after the
fact (#3978). A missing fragment is discovered only when someone reads the
release diff, if they do.

## Options

Each is measured, or priced, against the last 50 PRs merged to `main` (#3916 to
#3973, 2026-11-15). Of those 50, 16 carried a fragment.

### (a) A PR that touches shipped sources needs a fragment, or a `Changelog: none` trailer that says why

Shipped sources are `packages/`, `cmake/` and `book/`. Measured: (a) flags **10
of 50**.

| PR | Title (short) | Verdict |
| --- | --- | --- |
| #3951 | play_launch bump: a `<param from>` file is a model input | correct (build behaviour a user sees) |
| #3943, #3942, #3930, #3916 | book pages (XRCE reconnect, stack check fix, Zephyr Kconfig) | correct only if book additions count as a "Documentation" entry |
| #3964, #3944 | book release-notes pages | false: they ARE the release notes |
| #3955 | Renode twin cleanup test (`packages/testing`) | false: test-only |
| #3940 | nightly board cell selection (`packages/testing`) | false: CI-only |
| #3946 | gate.yml drops its schedule (touches `book/src`) | false: CI |

So 1 to 5 of the 10 flags are correct. Excluding `packages/testing/` and the
release-notes pages (`book/src/release-notes/`) drops the 4 clear false
positives. That leaves 6 flags: #3951, the 4 book pages, and #3946, which is
a judgement call. #3883 itself would have been flagged, since it touched
`packages/` and had no fragment. Measured misses are 0: the 9 `fix`/`feat` PRs
that (a) skips touch only `scripts/`, `just/`, `.github/`, `ci/` or
`docs/`, all contributor tooling.

Its cost is one trailer line on each internal-only PR that touches shipped
paths. The paths would need a stated exclusion list, and that list can drift.

### (b) A fragment is required only for `feat`, `fix` and `breaking` commit types

Measured: (b) flags **10 of 50**: #3940 and the 9 tooling PRs listed under
(a). About **0 of 10 are correct**: every one is contributor tooling (`scripts/check`,
fixtures, the gate lock, the runner image). It also misses the user-visible
`chore`/`docs` changes (#3951 and the book pages). In this tree a commit's type
says what kind of change it is, not who sees it, so (b) is the worst of the
three.

### (c) The release audit becomes a script that reports, and refuses nothing

The script lists the user-facing commits merged since the last tag that have no
fragment. It would use (a)'s path rule plus exclusions as a selector, and its
output feeds the pre-cut review. It never blocks a PR, so a false positive
costs a line in a report, not a contributor's time. A miss is caught at release
time, as #3883 was, rather than at merge time. It also suits a parked release
phase, because nothing in the merge queue depends on it.

## Recommendation (for the ruling, not decided here)

The measured results favour (c) now, and (a) with the exclusions later if the
report keeps finding misses. (b) is not worth building.
