---
id: 1639
title: "Renaming a component leaves its `<leaf>/metadata/<old>.json` behind, and the leaf road reads it: phantom subscriptions, refused rows, and a 20x executor arena"
status: open
type: bug
area: [cli, build, executor]
severity: medium
found: 2026-10-02
related: [issue-1340, issue-1522, issue-1594, phase-454, phase-457, rfc-0100]
---

## What happens

A cargo leaf's metadata probe writes one source-metadata file per component
into `<leaf>/metadata/<component>.json` (gitignored:
`.gitignore:207` `examples/**/metadata/*.json`, `:214`
`packages/testing/**/metadata/*.json`). The planner then reads EVERY `*.json`
in that directory — `orchestration/planner.rs` `metadata_paths()`,
`fs::read_dir(metadata_dir)` filtered on the extension — with no check that
the file still names a component the image declares.

So renaming a component (`[[component]] name` in `system.toml`, plus the
`Node::NAME`) leaves the old file in place, and the next `nros sync` composes
the old component's endpoints into the image beside the new one's.

## Measured

While writing issue 1340's fixture
(`packages/testing/nros-tests/bins/in-place-subscriptions`) the component was
first `five_listener` (five subscriptions) and then renamed `eight_listener`
(eight). After the rename, with no other change, `nros sync && nros build
native`:

- `build/nros/sizing/native.toml`: **13** `[[endpoint]]` rows and
  `subscription_entities = 13` — the eight real ones plus the five from
  `metadata/five_listener.json`;
- the five phantom rows are refused (`the contract for this image describes no
  subscription for component five_listener`), so each keeps a full receive
  region;
- the executor arena derives to **`ARENA_SIZE = 162,936`** where the same image
  with the stale file removed derives to **10,240**.

Nothing in the output says a component no longer exists; the warning reads as
a contract mistake about a component the author has already deleted.

## Why it matters beyond this fixture

The leaf road is the one road whose descriptor states every field (CLAUDE.md,
"THREE producers, one composer"), and its counts size the node table, the
subscription pools and the arena. A stale sidecar OVER-sizes (safe, wasteful,
and here 16x), but it also adds entities to `subscription_entities` that the
image never registers, so any check comparing declared against registered
counts reads the image as wrong.

## What would fix it

The planner should read only the sidecars of components the leaf DECLARES
(`workspace.component_declarations()` already enumerates them), or the probe
step should remove `metadata/*.json` files whose component is no longer
declared — and say so. A test: a leaf with a sidecar for an undeclared
component composes a descriptor without that component's rows.

Not fixed here: issue 1340's fixture work found it and worked around it by
deleting the stale file (`rm metadata/five_listener.json`, a gitignored build
output of the author's own rename).
