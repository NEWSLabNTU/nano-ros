---
id: 1091
title: "The api-parity ledger is the largest PR-conflict cluster left: 17 sorted
  JSON shards, and two branches adding disjoint keys collide with certainty"
status: resolved
resolved: 2026-10-26
type: tech-debt
area: tooling, docs
related: [0883, 0884, 1071, 1072, phase-396, phase-395, phase-516]
---

**Home: [phase-516](../../roadmap/phase-516-low-severity-issues-with-no-home.md) W2** since 2026-10-25. Before that, it stood on its own from 2026-10-24, when [phase-395](../../roadmap/archived/phase-395-dev-workflow-migration.md) was archived with every remaining wave either landed or deliberately not shipped. Before that, re-homed 2026-10-12 from phase-396, which is archived.

## The measurement

Re-measured 2026-09-05 against `origin/main`, over all 31 open pull requests
(the run that produced issue 1072's table):

| conflicting path | PRs |
| --- | --- |
| `docs/reference/api-parity-ledger/node.json` | 4 |
| `docs/reference/api-parity-ledger/other.json` | 3 |
| `docs/roadmap/phase-424-*.md` | 3 |
| `docs/reference/api-parity-ledger/graph.json` | 2 |
| `just/check.just` | 2 (fixed by 1072) |

As a group the ledger is **4 distinct PRs** (#329, #446, #471, #481) — the
largest remaining cluster, and now the largest full stop, since 1072 removed
`just/check.just`'s shared list.

## Why it conflicts, and why it will keep conflicting

The shards are written by `scripts/api-parity.py` with `sort_keys=True`, so a
new row lands at a position determined entirely by its KEY. Two branches adding
rows for adjacent items — which is the normal case, because the campaign closes
a feature at a time and a feature's items sort together — insert at the same
base line with no unchanged line between them to anchor on. Git cannot merge
that.

This is issue 1072's finding one directory over: **sorting converts NAME
correlation into POSITION correlation.** The shards were introduced precisely so
"one agent per lane can write without a rebase conflict against the others"
(`_doc` says so). Sharding by topic was the right first move and it is no longer
enough — `pubsub.json` alone holds 376 rows, `action.json` 290, `exec.json` 260.

Current sizes, 2504 rows over 17 files:

```
action 290   exec 260   pubsub 376   service 258   timer 259   other 255
param 134    qos 133    lifecycle 124   node 115   graph 89   serde 48
log 46       metadata 40   init 30   types 31   boot 16
```

## What does NOT work, already established

* **A `.gitattributes` merge driver.** Issue 0884 settled it: GitHub rebases
  merge-queue entries SERVER-SIDE, where `.gitattributes` drivers do not run.
  `merge=union` fixed `docs/issues/open.md` locally and not in the queue.
* **Generating the file and gitignoring it**, which is what 0884 did for
  `open.md`. Not available here: the ledger is AUTHORED. Its whole value is the
  hand-written `why` on each row — there is no generator to re-run.

## The shape that does work

Remove the shared file, the same answer 1072 reached for the gate registry and
0884 for the issue index. Two candidates:

1. **One file per row**, mirroring `docs/issues/` — a directory whose per-file
   design is exactly why issue FILINGS stopped conflicting while the generated
   index still did. 2504 small files; `api-parity.py` merges them the way it
   already merges 17.
2. **Shard harder by key prefix** — cheaper to land, and only pushes the
   collision out: `pubsub.json` splitting into `pubsub-c/cpp/rust` still puts
   every C++ pubsub row in one sorted file, and that is where the adjacent
   insertions are.

(1) is the one that actually ends it. (2) is a palliative and should be
recorded as such if chosen.

## Sequencing — do NOT start this while the cluster is open

The migration rewrites every ledger file, so it conflicts with all four PRs it
is meant to help, plus any phase-425 work in flight. Land the open ledger PRs
first, then migrate in one commit against a quiet tree.

## Acceptance

Two branches each adding a row whose key sorts adjacent to the other's merge
with no conflict, demonstrated rather than argued. `just check api-parity`
green, and `--self-test` still rejects a row in the wrong stage — the shard
identity that check depends on has to survive whatever layout replaces the
files.

## Resolution

Resolved 2026-10-26 by option (1), **one file per row**
([phase-516](../../roadmap/phase-516-low-severity-issues-with-no-home.md) W2).
None of the 3 pull requests open that day touched the ledger, so the
sequencing condition above held.

    docs/reference/api-parity-ledger/<topic>/<row file>.json   {"key": ..., "verdict": ..., ...}
    docs/reference/api-parity-ledger/<topic>/_doc.json         the topic's notes

* **The directory is the topic**, so the shard identity `--check` uses to
  refuse a row in the wrong stage (`_shard`) is read off it unchanged.
* **The row carries its own key; the file name is derived from it**
  (`scripts/api_parity/ledger_files.py` `row_filename`: `:` → `.`, any other
  byte outside `[A-Za-z0-9_-]` → `~XX`). Injective, so two keys never share a
  file, and the loader refuses a file whose name is not its key's. That is what
  keeps the duplicate-key protection meaningful: a second row for one key lands
  on the SAME path — an add/add conflict — not in a second file both sides
  keep. One key in two topic directories is refused outright; so are a
  top-level shard file (the retired layout, whose rows nothing would read), a
  row file with no `key`, and a `_`-prefixed field in a row file.
* **One reader.** `ledger_files.load` serves `api-parity.py`,
  `check-prelude-tiers.py` (which had its own loader),
  `check-ledger-key-spelling.py`, `check-ledger-orphan-refs.py` and
  `check-ledger-doc-single-home.py`; `roadmap_ledger_claims.py` and
  `gen-api-comparison.py` reach it through `api-parity.py`'s `load_ledger`.

**Measured.**

* Migration (`scripts/oneshot/migrate-ledger-one-file-per-row.py`): 3170 rows
  in 17 shards became 3169 row files plus 17 `_doc.json`. The one fewer is
  `cpp:spin_once`, which sat in BOTH `exec.json` and `node.json` with identical
  text; the old loader merged shards in sorted order and silently kept the
  last (`node`) — exactly the cross-file duplicate the new loader refuses.
* Lossless: the merged dict from origin/main's loader and from the new one have
  the same 3169 keys, identical `_shard` on all 3169, 14334 fields on both
  sides, and 3137 rows identical in every field but `_file`. The other 32
  differ ONLY where `why` prose named a retired shard FILE (`in boot.json` →
  `in boot/`, and one line-number citation `other.json:218 and :1070` rewritten
  as history) — `check-ledger-orphan-refs` resolves bare file names, so those
  would have cited nothing. All 17 `_doc` arrays are identical.
* Acceptance, demonstrated in a scratch clone: two branches add
  `cpp:Publisher::demo_a` / `cpp:Publisher::demo_b` (adjacent in the merged
  ledger). On origin/main's `pubsub.json`: `git rebase` → `Could not apply …
  add cpp:Publisher::demo_b`, `git merge` → `CONFLICT (content): Merge conflict
  in docs/reference/api-parity-ledger/pubsub.json`. On the new layout: `git
  rebase` → `Successfully rebased` (rc 0), `git merge --no-ff` of the
  un-rebased branch → `Merge made by the 'ort' strategy` (rc 0), and the loader
  reads 3171 rows with both present.
* `--self-test` plants a `qos` row under `node/` through the real writer and
  loader and requires it to come back misfiled (and the same row under `qos/`
  not to), plus each loader refusal and `row_filename` injectivity.
  Mutation-tested: `misfiled_shard` returning None, dropping the cross-topic
  check, dropping the file-name check, and folding `.` into `:` in the escaping
  each turn it red with exactly one failure.
