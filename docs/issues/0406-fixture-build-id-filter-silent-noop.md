---
id: 406
title: A fixture build narrowed to an id that matches nothing exits 0 having
  built nothing, and the two builders spell the narrowing differently
status: open
type: bug
area: build
related: [0393, 0196, 0351]
---

## Problem

`scripts/build/fixtures-build.sh` accepts `--id <id>` to build one manifest
row. If the id matches no row, it builds nothing, prints nothing, and exits 0:

```
$ bash scripts/build/fixtures-build.sh native rust --id workspace-rust-native-realtime
rc=0                       # 0.03s, no output — nothing was built

$ bash scripts/build/fixtures-build.sh native rust --id totally-made-up-nonsense
rc=0

$ bash scripts/build/fixtures-build.sh natve rust      # typo'd platform
rc=0
```

The first case is the one that costs time, because the id is real. It names a
`[[workspace_fixture]]`, and `fixtures-build.sh` only lists `[[fixture]]`
rows — workspace rows are built by a different script. A correct id, a correct
platform, a correct lang, and a clean exit, having done nothing. Observed
2026-08-03 while confirming the `realtime-*` fixtures still built; the run
looked like a 0.03s success.

The sibling builder is better but not good:

```
$ NROS_FIXTURE_ID=made-up bash scripts/build/workspace-fixtures-build.sh native rust
No workspace fixtures matched platform=native lang=rust id=made-up.
rc=0
```

It says so, then exits 0 anyway. In a sweep that line scrolls past among
hundreds.

## Why it matters here specifically

The whole point of `_require-fixtures` and the `.fixtures-built` stamp
(issue 0393) is that a test run can ask "does what was built cover what I am
about to run?" A build step that exits 0 without building satisfies every
caller that checks exit status, which is the same failure shape as issue 0351
(a stamp certifying a build stage that had stopped working) and issue 0196 (a
probe whose coverage was narrower than the rule it enforced).

It also hides typos in exactly the place they are most likely: a hand-run
narrowing while iterating on one fixture.

## The distinction a fix has to keep

Zero matched rows is NOT always an error. Sweeps legitimately hit empty
coordinates:

```
threadx-linux/rust  rows=13
threadx-linux/c     rows=12
threadx-linux/cpp   rows=12
threadx-linux/mixed rows=0     <- routine, not a mistake
```

The per-platform recipes iterate all four languages, so failing on an empty
`(platform, lang)` would break every sweep. The rule that separates them:

> An EXPLICIT id filter that matches nothing is always a user error. An
> unmatched `(platform, lang)` during a sweep is routine.

So: when `--id` / `NROS_FIXTURE_ID` is set and selects zero rows, fail loud
and name the id. Leave the unfiltered case exactly as it is.

## Second half: one narrowing, two spellings

The two builders disagree on how to say the same thing:

| script | narrowing |
| --- | --- |
| `fixtures-build.sh` | `--id <id>` (a flag) |
| `workspace-fixtures-build.sh` | `NROS_FIXTURE_ID=<id>` (an env var) |

Neither rejects the other's spelling — `fixtures-build.sh` ignores
`NROS_FIXTURE_ID`, and `workspace-fixtures-build.sh` would reject `--id` as an
unknown argument only because it takes no flags at all. Passing the wrong one
to the wrong script is precisely how the silent no-op above was reached.

Worth converging on one spelling (both scripts already read
`NROS_FIXTURE_COORDS` from the env for the lane narrowing, which argues for
the env var), or at minimum making each script reject the other's so the
mistake is loud rather than silent.

## Direction

1. In both scripts: if an id filter is set and the row listing is empty, print
   the id and the coordinates it was filtered against, and exit non-zero.
2. Make the unfiltered zero-row case stay silent and successful — that is the
   sweep's normal path.
3. Converge the two spellings, or cross-reject them.

The gate to check afterwards, per the issue-0196 rule: whether anything in
`just/*.just` currently relies on an id filter matching nothing. Nothing
should, but a sweep that passes today by accident would turn red, and that
red is the bug becoming visible rather than a new one.
