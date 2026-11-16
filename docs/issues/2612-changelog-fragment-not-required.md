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

**Ruled 2026-11-15 (user): option (c) now, (a) later.** (c) is done (below);
the issue stays open for (a). **Home:** no active phase owns release tooling. The release phases (phase-431, phase-443)
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

## Recommendation (made before the ruling)

The measured results favour (c) now, and (a) with the exclusions later if the
report keeps finding misses. (b) is not worth building.

## Ruling (2026-11-15)

The user chose (c) now and (a) later: a report-only script for the release's
pre-cut audit, wired into the release dry run, which becomes a pull request
check once it has stayed quiet across releases.

## Done: (c)

`scripts/changelog-audit.py` lists every pull request merged since the last
`nros-*` tag (the nearest one reachable by history) that touches a shipped
path and carries no `changelog.d/` fragment in any of its commits and no
`Changelog: none` trailer.

- **Shipped:** `packages/`, `cmake/`, `book/`, except `packages/testing/`, the
  release-notes pages (`book/src/migrating/releases/`, not
  `book/src/release-notes/` as written above) and the book's navigation files
  (`SUMMARY.md`, `page-map.toml`, the releases index). Without the last
  exclusion every notes-page PR flagged itself for adding its own entry to the
  table of contents (#3944, #3964).
- **Grouped by PR** through GitHub's commit-to-PR association, so a fragment
  added in a PR's later commit covers its earlier ones. With no `gh`, one row
  per commit.
- **Never fails.** `release-nros.yml` runs it on every dispatch, dry runs
  included, in its own `changelog-audit` job: full history, `continue-on-error`,
  nothing `needs` it, and the table goes to the job summary.
- **Gate:** `just check changelog-audit` (fast line) runs the selector's cases on
  a scratch repository; each guard was mutation-checked.

Since `nros-0.6.0-nros10` it reports 9 of 58 PRs, a superset of the 6 measured
above: #3883 (fragment added later, #3978), the four book-page PRs, #3951,
#3946, and #3910 and #3912, which predate the 50-PR sample. The nros11 notes
reviewer judged none of the eight beyond #3883 to need a notes line: book pages
publish from `main` rather than with a release, and #3910's and #3946's
`packages/boards/` edits are probe cadence metadata. That is the open question
for (a): whether book-only PRs count as shipped.


**Ruled 2026-11-16 (user, via the manager): book- and docs-only PRs do not
count.** A group whose every path is documentation by
`scripts/ci/gate-tier.py`'s own `tier()` (book/, book-internals/, any `.md` no
Rust source reads) is skipped and COUNTED in the report's headline and job
summary, never dropped silently. The audit imports that predicate rather than
keeping a second one. Same change: the PR lookup is one aliased GraphQL query
per 100 commits instead of one REST call per commit. Since `nros-0.6.0-nros11`
(19 commits, 16 PRs, one call): 2 flagged, 1 skipped as docs-only.
