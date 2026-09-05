---
id: 1095
title: "The api-parity ledger's schema doc is copied into all 17 shards, so a schema change conflicts in 17 files at once"
status: open
type: bug
area: tooling
related: [issue-0883, issue-0884]
---

## Problem

`docs/reference/api-parity-ledger/` is 17 JSON shards, and **every one carries
its own copy of the same 76-line `_doc` schema description** — what a verdict
means, how a key is spelled, which fields are required. Measured:

```
distinct _doc variants: 2
  x16  action, boot, exec, graph, init, lifecycle, log, metadata,
       node, other, param, pubsub, serde, service, timer, types
  x1   qos          (the shared block + a 17-line qos addendum)
```

The block has GROWN under duplication, which is the cost made visible: it was
34 lines a few days ago and is 76 now, because `their-rename` (commit
`864ba17c1`) had to write its ~42 new lines into all seventeen files.

So the schema has 17 homes. Changing it means editing 17 files, and two
concurrent PRs that both touch the schema conflict in **up to 17 paths** — not
because they disagree about code, but because they each rewrote the same
paragraph in seventeen places.

## What it actually cost

Three open PRs — #329, #446, #471, all phase-417 — cannot rebase onto `main`
today. Each hits the same two-layer sequence:

1. `scripts/api-parity.py`, one hunk, genuinely mechanical: `main` adds
   `validate_their_rename(...)`, the branch adds validation for a new optional
   `disposition` field. Independent additions; a union resolves it.
2. **`graph.json` + `node.json` + `serde.json`** — competing rewrites of the
   `_doc` block. `main` documents the rename vocabulary, the branches document
   `disposition`. Resolving layer 1 just moves each PR to layer 2.

That second layer is not a merge, it is an authoring decision about what the
ledger's contract now says — and it has to be taken once and then copied into
every shard by hand, which is how the copies drift in the first place.

`qos.json` shows the drift has already started: it is the only shard whose
`_doc` differs, because a phase-379 note was added there and nowhere else. That
note is legitimately qos-specific; the point is that nothing distinguishes
"shard-specific note" from "schema description" today, so nothing stops the
shared half diverging too.

## Why this is the 0883/0884 class

Issues 0883/0884 are the same shape one surface over: a file that every PR
touches becomes the only conflicting path, and the merge queue serialises on it.
There the file was generated and the fix was to stop tracking it. Here the file
is hand-written, but the duplication is mechanical — 16 byte-identical copies —
so the fix is the same in spirit: **one home for the fact, and a pointer from
everywhere else.**

## Fix

Extract the shared schema description to a single document; each shard's `_doc`
keeps only a pointer plus any genuinely shard-specific notes (qos's addendum
stays with qos). Gate that no shard reintroduces a full copy.

Then a schema change is one file, and the three phase-417 PRs conflict on
`api-parity.py` alone — which is mechanical.

## Acceptance

* The schema description exists in exactly one file.
* No shard's `_doc` contains the schema vocabulary; a gate fails if one does.
* `scripts/api-parity.py --self-test` and `just check api-parity` still pass.
