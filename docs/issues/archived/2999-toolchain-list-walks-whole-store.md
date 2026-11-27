---
id: 2999
title: "`nros toolchain list` walks every file in the store on every call"
status: resolved
type: bug
severity: medium
area: [cli, store]
related: [phase-569]
found: 2026-11-27
resolved: 2026-11-27
resolved_in: "fix-toolchain-list-walk"
---

## What happened

`nros toolchain list` (and `nros store list`) took over 120 s on host 139.
It walked every file of every store entry to size it and to date its last
read, on every call, before printing anything.

Measured on 139, where `/home` is a shared disk under load (load average about
10, other users' jobs on the same filesystem):

| | files | wall |
| --- | --- | --- |
| a full walk of `~/.nros` (56 GB) | 510,871 | 1,023 s cold, 0.6 s warm |

So the cost is the page cache. On a store that is not in cache (the first
call after a while, or a busy disk), a listing waits for a full walk.

## Cause

`store::scan` measured each entry while building it (`build_entry` called
`measure`, a `walkdir` over the entry). The listing needs no measurement to
say what is installed: an entry's category, name, version and state come from
directory names and the provenance file. Only the SIZE, LAST-USED and EVIDENCE
columns need the walk, and only `gc`, which decides by age and size, needs it
to act.

## Fix

- `store::scan_heads` returns each entry unmeasured (`EntryHead`), and
  `scan` is `scan_heads` then `EntryHead::measure`.
- `nros toolchain list` prints STATE and ENTRY from `scan_heads`; `--usage`
  adds the walk and the full table as before. `gc` and `remove` keep
  measuring. The "installed versions" line of `toolchain remove` uses
  `scan_heads`.

Measured after: the default listing 5.5 s cold and under 0.01 s warm on the
same store; `--usage` 0.74 s warm.

## Test

`cmd::store::tests::list_without_usage_walks_no_entry` lists a store with two
entries through the same function `list` calls and asserts no entry was
measured (a counter in `measure`). Its negative control measures the same
store with `scan` and counts every entry.
