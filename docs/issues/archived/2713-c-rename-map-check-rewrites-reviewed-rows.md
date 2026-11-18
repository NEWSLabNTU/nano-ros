---
id: 2713
title: "`gen-c-rename-map.py --check` reads the reviewed C rename map as stale on every host, and a regeneration deletes its history and reviewed classes"
status: resolved
resolved_in: "the change that files this issue"
type: bug
severity: low
area: [c-api, tooling, gates]
related: [1226, 2554]
found: 2026-11-18
---

**Home:** no phase (phase-492, which built the map, is archived).

## What happened

`python3 scripts/gen-c-rename-map.py --check` reported
`docs/reference/c-api-rename-map.tsv` STALE on main, and no gate ran it. The
AUO book session saw that regenerating would delete many `ours` rows
(`NROS_ACTION_*_STATE_*` and others), and it looked host-dependent, like
issue 2554.

## Cause: not the host, the generator's model of the file

The generator's file set is `git ls-files` over the nros-c and seam include
trees, so it is the same on every host. Measured on main: regenerating writes
the SAME result here as on the AUO host, 23 rows added and 740 deleted. The
map is not a generated file. It is reviewed, then committed, and it holds
three kinds of row:

| rows | what they are |
| --- | --- |
| 717 | the rename history: names phase-492 W3/W5 already renamed (`NROS_ACTION_CLIENT_STATE_INITIALIZED` → `RCLC_ACTION_CLIENT_STATE_INITIALIZED`), which the headers no longer declare; a port from the old API reads them |
| 22 | reviewers' classes that differ from the generator's guess (`NROS_LEVEL_STACK_BYTES` is reviewed as `hook`, guessed `ours`; `NROS_RMW_REQUEST_ID_T_DEFINED` `build`, guessed `seam`) |
| the rest | rows the generator would write the same way |

`--check` compared the committed file with `build_map()` written from
scratch, so it could only pass on the day the map was first generated. A
regeneration deleted the history and undid the review, on any host.
Unmapped names, the thing that CAN go stale, were 0 on main.

## Fix

The generator merges. It keeps every committed row as committed, appends a
generated row only for a name the map does not classify yet, and keeps the
file sorted by old name. `--check` fails only on such an unclassified name,
listing each with its suggested row. On main the run is a no-op: byte-identical
file, 985 reviewed rows, 0 new.

Negative control: a `#define NROS_PROBE_UNMAPPED_NAME 1` added to a tracked
header makes `--check` fail naming it ("would be: ours ->
RCLC_PROBE_UNMAPPED_NAME"), and a run appends exactly that row.

The check now runs: `just check c-rename-map`, a fast-line gate (issue
1226's shape, a check that worked and never ran). It complements
`c-api-nros-names`, which holds the same rule over the nros-c include tree
alone. The generator also reads the port seam headers.

Rows added by hand for new names (the AUO session's seven `NROS_DIAG` rows)
are reviewed rows like any other: a later run keeps them.

Sweep: `git grep -n 'gen-c-rename-map' -- just scripts .config` (the
generator's one consumer of its output is `check-c-api-nros-names.py`, which
reads the map and is unchanged).
