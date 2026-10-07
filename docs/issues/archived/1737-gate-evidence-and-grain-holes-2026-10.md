---
id: 1737
title: "Six gates accept the wrong evidence or the wrong grain (W3/W6/W8 class, 2026-10-07 re-audit)"
status: resolved
type: bug
area: [tooling, ci, testing]
severity: medium
found: 2026-10-07
related: [phase-472, issue-1735, issue-1736]
resolved_in: "gate-reach follow-up, item 3"
---

## What the re-audit measured

From the 2026-10-07 gate-reach re-audit
([audit-findings-2026-10-07](../../development/audit-findings-2026-10-07.md)).
Every mutation passed (rc 0); every control failed (rc 1).

| gate | class | mutation that passes | control |
| --- | --- | --- | --- |
| `tier-has-ci-owner` | W3 (text as evidence) | both `just ci tier1 …` invocations in `host-tests.yml` replaced; Tier 1 is still "owned" by the ARGUMENT strings of `disk-report.sh "before just ci tier1"` / `reclaim-disk.sh` | reword those strings |
| `px4-archive-header-pairing` (claim 1) | W3 | the C-header `nros_assert_archive_pairs_with_header(` renamed `message(STATUS` — "paired, by byte scan" sees the header path | delete the block |
| `ps-zombie-blind` | rule half-checked | `subtree-guard.sh` keeps `stat=` and drops `$3 !~ /^Z/`; the gate checks the column is requested, never that Z is excluded | drop `stat=` |
| `dds-isolation-symmetry` | W6 grain | drop the pin in `spawn_bridge` (the `Command` helper the ros2-peer test calls); the per-fn rule needs `Command::new(` in the SAME fn as the peer | drop an inline pin |
| `lane-scope-consumers` | W6 grain | `entry_e2e.rs` drops the narrowing `filter(admits)` and keeps the out-of-lane REPORT `admits` call; rule (b) is "the file calls admits" | drop both calls |
| `one-producer-per-tool` | W8 exemption | `install-corrosion` forwards `nros setup --tool corrosion` AND curls corrosion itself — the forward excuses producing the same tool, i.e. the second producer | curl `ninja` in the same recipe |

## Fix direction

Key each predicate on the thing the rule names: an owner is a COMMAND word at
run depth (W1 `command_lines`), a pairing is a CALL (`per_item.cmake_calls`), a
zombie filter is a predicate on the stat column, a pin follows same-file
`Command` helpers, a forward excuses only the absence of a download — and give
each the selftest row its mutation shows is missing.

## Resolution

Each predicate now keys on the thing the rule names, with the selftest row its
mutation showed was missing.

| gate | change (helper) | mutation | old rc | new rc |
| --- | --- | --- | --- | --- |
| `tier-has-ci-owner` | an owner is `just <tier>` as the COMMAND WORD of a statement (`shell_statements`, issue 1738's helper), with `VAR=`/`env`/`time`/`timeout N` peeled — never a word inside an argument | both live `just ci tier1 …` invocations (`host-tests.yml`, `run-matrix.yml`) replaced by `true`; only the `disk-report.sh "before just ci tier1 gates"` argument strings remain | 0 | 1 |
| `px4-archive-header-pairing` (claim 1) | the wiring scan is per CALL (`per_item.cmake_calls` + `cmake_keyword_items` over `comments`-stripped code): each header must be the HEADER of a real `nros_assert_archive_pairs_with_header(` call | the C-header call renamed `message(STATUS` | 0 | 1 |
| `ps-zombie-blind` | the second half of the rule: a group scan must DROP Z rows in its consumer (pipe stages / Rust `.filter`, or the loop body a `done < <(ps …)` feeds); a stdout-to-/dev/null capability probe reads no rows | `subtree-guard.sh` keeps `stat=` and drops `$3 !~ /^Z/` | 0 | 1 |
| `dds-isolation-symmetry` | a peer fn's same-file SPAWN HELPERS are its processes too (`per_item.blocks`); a helper handed a zenoh locator and no DDS domain is a zenoh session, not on the bus | drop the pin inside `spawn_bridge` | 0 | 1 |
| `lane-scope-consumers` | rule (b) is a NARROWING use: a positive `filter(\|c\| admits(…))` or `if !admits(…) { … continue/return }` (`per_item.blocks`), not a report-only `filter(!admits)` | `entry_e2e.rs` drops the narrowing filter, keeps the report | 0 | 1 |
| `one-producer-per-tool` | a forward no longer excuses a producer line of the SAME tool (it only ever meant "no download here") | `install-corrosion` forwards AND curls corrosion | 0 | 1 |
| controls | | drop both `admits` calls; curl `ninja` beside the forward | 1 | 1 |

No live tree defect surfaced; every gate is green on the tree.
