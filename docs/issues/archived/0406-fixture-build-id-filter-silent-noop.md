---
id: 406
title: A fixture build narrowed to an id that matches nothing exits 0 having
  built nothing, and the two builders spell the narrowing differently
status: resolved
type: bug
area: build
related: [0393, 0196, 0351]
resolved_in: check-fixture-id-guard (scripts/build/fixture-id-guard.sh)
---

## What was wrong

Every fixture builder answered an empty row set the same way: exit 0, having
built nothing.

```
$ bash scripts/build/fixtures-build.sh native rust --id workspace-rust-native-realtime
rc=0        # 0.03s, no output — nothing built
```

The id is real. It names a `[[workspace_fixture]]`, and that script lists only
`[[fixture]]` rows. Correct platform, correct lang, correct id, clean exit, no
work. A typo'd id and a typo'd platform behaved identically. The sibling
builder printed `No workspace fixtures matched…` and exited 0 anyway.

Found 2026-08-03 while confirming the `realtime-*` fixtures still built. Same
shape as 0351 and 0196: a step that satisfies every exit-status check without
doing anything, feeding the `.fixtures-built` stamp (0393) whose purpose is
answering "does what was built cover what I am about to run?"

## Why the obvious fix was wrong

"Zero rows = error" breaks every sweep. `threadx-linux/mixed` legitimately has
0 rows and the per-platform recipes iterate all four languages.

What separates an error from a normal empty pass is HOW the filter arrived —
and the two spellings the issue flagged as inconsistent turned out to already
mean different things, so they were kept and documented rather than merged:

| filter | matched nothing | why |
| --- | --- | --- |
| `--id <id>` | **exit 2** | targets THIS builder; nothing else will run |
| `NROS_FIXTURE_ID=<id>` | note, exit 0 | sweep-wide, crosses builders; stages legitimately miss |
| either, id in NO table | **exit 2** | a typo no stage in any sweep could build |
| no id filter, empty coordinate | silent, exit 0 | routine |

## Fix

`scripts/build/fixture-id-guard.sh` — one place that decides what an empty row
set means, sourced by all three builders (`fixtures-build.sh`,
`workspace-fixtures-build.sh`, `compile-check-fixtures.sh`). Failures name the
row's own coordinates and print the invocation that would build it, instead of
echoing back the ones that just failed.

`fixtures-manifest.py` gained `describe-id` (classify an id across all three
tables) and `list-platforms` (a vocabulary to reject `natve`), keeping manifest
parsing in one place. `workspace-fixtures-build.sh` learned `--id`.

Gated by `check-fixture-id-guard` in `check-fast`, buildless. Verified by
neutering the guard: 7 of 9 cases fail, including the original symptom
(`expected rc=2, got rc=0`, empty output).
