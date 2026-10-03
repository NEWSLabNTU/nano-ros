---
id: 1639
title: "Renaming a component leaves its `<leaf>/metadata/<old>.json` behind, and the leaf road reads it: phantom subscriptions, refused rows, and a 20x executor arena"
status: resolved
type: bug
area: [cli, build, executor]
severity: medium
found: 2026-10-02
resolved_in: 2026-10-03
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

## Resolution

**`nros sync` prunes them, at the one writer** —
`metadata_refresh::prune_undeclared_sidecars`, run at the head of
`refresh_stale_sidecars` (before anything is probed, so no read in the same
pass can see a stale file either). Pruning was chosen over filtering at the
readers because there are four of them, not one: `leaf_entity_env::
inventory_for_leaf` (the cargo-leaf sizing descriptor — the road measured
here), and `Workspace::source_metadata_files` behind the planner's
`metadata_paths`, `model_ingest` and `nros metadata`. Removing the file makes
the directory mean what all four already assume.

What it removes, and what it does not:

- only inside a directory a DECLARED sidecar lives in
  (`ComponentDeclaration::source_metadata_path`'s parent);
- only a `*.json` no declaration names **and** that carries
  `SourceMetadataProvenance` — the stamp `stamp_provenance` writes on every
  successful probe, so the file is provably the probe's;
- with it, its `.json.unprobeable` negative-cache marker; an orphaned marker
  goes too;
- an UNSTAMPED undeclared `*.json` (a hand-written fixture) is kept and
  reported (`sync: source metadata — <file> names no declared component and
  was not written by the probe, so it was kept; every reader of the directory
  still counts it`), because deleting what this pass did not write is not its
  call, and the readers still see it.

Sync prints `sync: source metadata — removed <file> (its component is no
longer declared)` per removal, before its early return.

### Measured

`packages/testing/nros-tests/bins/in-place-subscriptions` (component
`eight_listener`, 8 subscriptions), with the issue's stale file recreated as
`metadata/five_listener.json` (the probe's own sidecar, renamed, cut to 5
subscriptions, provenance intact):

| | `[[endpoint]]` rows | `subscription_entities` | `ARENA_SIZE` |
| --- | --- | --- | --- |
| no stale file | 8 | 8 | 10,240 |
| stale file, `origin/main` CLI | **13** | **13** | **162,936** (+ five "describes no subscription for component `five_listener`" warnings) |
| stale file, this branch | 8 | 8 | 10,240 — sync printed `removed …/metadata/five_listener.json` |

The `origin/main` row reproduces the issue's numbers exactly.

Unit test `a_renamed_components_old_sidecar_is_pruned`: asserts the leaf
reader globs all three sidecars BEFORE (the precondition), that only the
stamped undeclared one and its marker are removed, that the unstamped one is
kept and reported, that an orphan marker goes, that `inventory_for_leaf` then
reads two, and that a second pass is a no-op.

### Not measured

- A C/C++ component rename on the cmake probe road. The prune keys on
  `source_metadata_path`, which both roads share, and the C/C++ probe stamps
  the same provenance, but no cmake leaf was renamed and rebuilt here.
- `nros build` without a preceding `nros sync`: the prune is a sync step, so a
  build that does not run sync's metadata refresh still reads whatever the
  directory holds.
