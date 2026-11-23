---
id: 2911
title: "The census pre-pass never takes the census of a compile-check row configured by `nros build`, so `west_owned_entry_census_*` refuse on a fresh tree"
status: open
type: bug
area: [ci, build, census]
severity: high
found: 2026-11-24
related: [phase-478, 2862, 1419, 2390, 2436]
---

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)**
(the owned west entry fixtures of issue 2862 are island-shaped).

## What happens

PR #4646 (issue 2862) added two `[[compile_check_fixture]]` rows,
`west_owned_entry_census_flat` and `west_owned_entry_census_nested`. Each is a
`west-configure` row with `west_nros_image = "talker_bringup:zephyr"`, so
`scripts/build/west-fixtures.sh` configures it through
`nros build talker_bringup:zephyr --cmake-only`. The image builds the owned
entry `zephyr_c_talker`, which has no SystemModel and is sized by the census
of its own program. The configure refuses when that census is missing:

```
`talker_bringup:zephyr` builds the owned entry `zephyr_c_talker`, which is
sized by its census, and its census is missing (no census at
<fixture>/build/nros/census/owned/talker_bringup/zephyr.census.json).
Take it: nros dev image entity-census take --image talker_bringup:zephyr
```

On the self-hosted tier-1/tier-2 runner both rows failed this way (post-submit
run 45129078867 on main after PR #4646, `zephyr.log` of
`build-test-fixtures`). The
run's `build/census-prepass` logs name neither fixture.

## Root cause

The census pre-pass (`scripts/build/census-prepass.sh`, issue 1419) takes
every image census from `fixtures-manifest.py census-images`. That listing read
only the `[[workspace_fixture]]` table. A `[[compile_check_fixture]]` row that
is configured by `nros build <bringup>:<image>` (issue 2436's
`west_nros_image`) is a cross image configure too, but no listing named it.
After the pre-pass, `build-test-fixtures` exports `NROS_CENSUS_PREPASS=done`,
and nothing else takes the census before `west-fixtures.sh` configures the row.

`west_owned_entry_workspace_module` and `west_hand_written_c_entry_nros_build`
passed only because their images are sized by a model and read no census. The
first row whose image needs one failed.

## Why the local gate passed

`just ci gate` builds no fixtures; the Zephyr fixture build that runs the
pre-pass runs only in the tier-1/tier-2 lanes on the self-hosted runner. A
direct `scripts/build/west-fixtures.sh` run (no `NROS_CENSUS_PREPASS`) lets
`nros build` take the census itself, and under the default probe policy a
missing census is only a warning, so the row configured locally. Reproduced
on a fresh worktree with `NROS_CENSUS_PREPASS=done NROS_PROBE_FAILURE=refuse`:
`MISSING build.ninja` with the runner's error.

## Fix

One predicate, `compile_check_nros_image(entry)` in
`scripts/build/fixtures-manifest.py`: the image a `west-*` compile-check row
is configured through, or empty. `compile_check_record` emits it as the field
`west-fixtures.sh` runs `nros build` on, and `census-images` now lists every
compile-check row it names, narrowed by the same rules `list-compile-checks`
uses (platform, lane coordinates, supplied job modules). So every row whose
configure builds an image has that image's census taken first, and a future
row cannot miss it; `take` answers "nothing to take" for an image that needs
no census.

`tests/census-images-compile-check-rows-tests.sh` (`just check
census-images-compile-check-rows`) proves it against the real manifest: every
row the predicate names is listed, including both census rows, the
`--platform zephyr` narrowing keeps them, and other platforms drop them.
