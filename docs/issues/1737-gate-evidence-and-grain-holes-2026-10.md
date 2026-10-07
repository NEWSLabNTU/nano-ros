---
id: 1737
title: "Six gates accept the wrong evidence or the wrong grain (W3/W6/W8 class, 2026-10-07 re-audit)"
status: open
type: bug
area: [tooling, ci, testing]
severity: medium
found: 2026-10-07
related: [phase-472, issue-1735, issue-1736]
---

## What the re-audit measured

From the 2026-10-07 gate-reach re-audit
([audit-findings-2026-10-07](../development/audit-findings-2026-10-07.md)).
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
