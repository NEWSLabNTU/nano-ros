---
id: 1091
title: "The api-parity ledger is the largest PR-conflict cluster left: 17 sorted
  JSON shards, and two branches adding disjoint keys collide with certainty"
status: open
type: tech-debt
area: tooling, docs
related: [0883, 0884, 1071, 1072]
---

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
