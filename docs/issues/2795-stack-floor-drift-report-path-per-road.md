---
id: 2795
title: "The stack-floor drift lane finds a build's report by globbing its build root, because the report-path rule spells only the cmake road"
status: open
type: tech-debt
severity: low
area: [stack-floor, ci]
related: [issue-2400]
found: 2026-11-22
---

**Home:** [phase-583](../roadmap/archived/phase-583-owned-entry-executables.md) W4,
which adds the owned-entry program key `<entry>/<exe>@<board>` and the one
report-path rule, `report_path(build_dir, key)` in
`scripts/nros-stack-floor.py`.

## What happens

`scripts/stack-floor-drift.py` finds the report a build wrote by globbing
`**/report.json` under the image's build root (`_reports`) and keeping the one
whose own `image` is the row's key (`pick_report`). The agreed rule is that
every reader resolves the path through `report_path` instead, so a report at
the wrong path reads as missing, not as found.

`report_path` spells the cmake road's location only
(`<build>/nros/stack-floor/[<exe>/]report.json`). Drift also re-walks rows
built on the cargo road (`<target dir>/nros-stack-floor/report.json`) and the
west road, so swapping the glob for it needs a per-road rule first, fed by
the build dir the build states (`nros run <image> --dry-run --json` names
`build_dir`).

## Why it is safe meanwhile

`--validate-table` refuses a program key in every row drift reads, so drift
never meets a per-program report, and `pick_report` matches the report's own
`image`, so it cannot take one image's report for another's.

## Fix

A per-road `report_path`, then drift's build route for program keys (the
`[[workspace_fixture]]` row whose image names the entry), then lift the
validation refusal.
