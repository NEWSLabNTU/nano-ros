---
id: 2893
title: "Every scheduled fixture build stops at `unknown lane 'lane=tier2'`: a lane helper refuses the `lane=` spelling a recipe forwards"
status: resolved
type: bug
severity: high
area: [ci, fixtures, build]
related: [issue-2831]
found: 2026-11-23
resolved: 2026-11-23
resolved_in: "the change that archives this issue"
---

## What fails

Scheduled run-matrix run 45124029656 (main `b61082063c`, 2026-11-23): the
tier 1, tier 2 and tier 2 nightly jobs all stop before any fixture is built:

```text
fixture-lane: unknown lane 'lane=tier2' — expected one of: all linux tier1 tier2 tier2-nightly merge
error: recipe `_require-owned-provisioned-roots` failed with exit code 2
error: recipe `_build-scope` failed with exit code 2
```

No cell runs, so the lanes give no verdict, and whatever they would have
found stays hidden behind this.

## Why

- `_build-scope` runs `just build-test-fixtures "lane=$tok"`. just passes the
  `name=value` form to a recipe verbatim, so the recipe's `lane` is the
  string `lane=tier2`. `nros_lane_arg` in `scripts/build/fixture-lane.sh` is
  the one normaliser for that, and the recipe bodies call it.
- 07db836b34 (issue 2831) made `_require-owned-provisioned-roots` a
  dependency taking the build's lane. It forwarded the raw value to
  `nros_lane_modules`, which validated it without normalising and refused.
- The recipe's own `!= "all"` test compared the raw value too, so `lane=all`
  would have taken the narrowed branch and looked up lane modules instead of
  running the full probe set.
- Six helpers had the same gap: `nros_lane_coords_file`,
  `nros_lane_unclaimed`, `nros_lane_build_lane`, `nros_lane_modules`,
  `nros_fixtures_stamp_write` and `nros_fixtures_stamp_require` each called
  `nros_lane_validate "$lane"` on a value they had not normalised. Only the
  first forwarder to reach one of them raw exposed it.

## Fix

Each of the six helpers sets its lane through `nros_lane_arg` at entry, so a
recipe may forward `{{lane}}` as it arrived. The recipe normalises before it
compares. One consequence: `nros_lane_arg ""` is `all`, so those helpers now
answer an empty lane as `all` where they used to refuse it. Every caller
passes a non-empty lane or relies on that default.

Gate `just check lane-arg-spelling` (`tests/lane-arg-spelling-tests.sh`,
buildless):

- every function in `fixture-lane.sh` that calls `nros_lane_validate "$lane"`
  sets it through `nros_lane_arg` first;
- `nros_lane_modules` and `_require-owned-provisioned-roots` take both
  `linux` and `lane=linux`;
- negative control: a copy of the helper with the normaliser removed refuses
  `lane=linux`.

Run against main's `justfile` and `fixture-lane.sh`, the test fails with
run 45124029656's exact error.

Sweep: `git grep -n 'nros_lane_validate "$lane"' scripts/build/fixture-lane.sh`
and `git grep -n -E '\{\{ *lane *\}\}' justfile just/`.

Filed by nros 175 (reserved `refs/issue-ids/2893`), who had the same fix
locally; the two were merged into this one.
