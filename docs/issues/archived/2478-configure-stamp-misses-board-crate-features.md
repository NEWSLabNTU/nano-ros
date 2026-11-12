---
id: 2478
title: "`nros configure` reports a leaf up to date while its selection crate forwards a board feature the board crate no longer has"
status: resolved
type: bug
severity: high
area: [cli, build]
found: 2026-11-13
related: [2480, 2485, phase-553]
resolved: 2026-11-13
resolved_in: "every configure-path read outside the project goes through `configure_stamp::read_external` and is a stamp input"
---

> **Resolved (2026-11-13).** A file outside the project that configure reads
> (a board crate's `Cargo.toml`, a board descriptor) is now recorded in the
> stamp with the digest of the bytes read, and the stamp is stale once one of
> them changes or goes. See "Fix".

## What

A leaf's generated selection crate (`<leaf>/build/nros/selection/Cargo.toml`)
forwards every feature of its board crate (`"<f>" = ["<board>/<f>"]`). When
the board crate's feature list changed, the selection crate was not
regenerated: `nros configure` printed `configure: up to date (1 image(s))`
and kept the old table, so a removed feature made cargo refuse the leaf:

```
package `baremetal_c_talker_nros_selection` depends on `nros-board-mps2-an385`
with feature `c-libc` but `nros-board-mps2-an385` does not have that feature
```

Measured 2026-11-13 in one worktree: a branch that added `c-libc` to
`nros-board-mps2-an385` (phase-553 W1) was built, then the worktree was
switched to a branch without it. `just ci gate` failed at step 7
(`check::linux-check`) on three leaves: the mps2-an385-baremetal C talker, C++
talker and Rust listener. `nros configure` in a leaf left the line in place;
`nros configure --force` removed it, and the leaf resolved. Nothing was wiped.

## Why

The configure stamp's file inputs were the project walk (`input_files`):
manifests, `system.toml`, launch and interface files UNDER the project.
Configure also reads files OUTSIDE it and copies what they say into its
output: the board crate's `[features]` (the selection pass-through, the
derived default features, the image link features, the backend derivation)
and the board descriptors. None of those was an input, so a change to one
left the stamp current over an answer derived from the old bytes. It is a
missing dependency edge, not a wipe case.

Issue 2480 is the same class from the other side: a stamp input
(`.leaf-selection`) that no configure on the reading road writes.

## Sweep

Every input `configure_stamp::currency` reads, against who writes it on each
road:

| Input | Written by | Road |
| --- | --- | --- |
| `build/nros/configured/<image>/stamp.toml` | configure (`record`, `record_planned`) | every road |
| `build/nros/configured/.leaf-selection` | configure and `leaf_settings::write_selection`, leaf only | leaf; **read on every road** (issue 2480, fixed) |
| project walk (`input_files`) | the user | every road |
| digest env (`AMENT_PREFIX_PATH`, `NROS_INTERFACE_SEARCH_PATH`, `NROS_MODEL_DIR`) | the caller | every road |
| the shared coordinate's stamp | configure | shared host configure |
| refused-model markers | `model_gate` | every road |
| outputs (`missing_output`) | configure, per road | each branch checks its own road |
| generated crates' `nros-generated.toml` (ament) | codegen | every road |
| metadata sidecars | the metadata refresh | every road |
| entity censuses | the census step | every road |
| files outside the project | the user, or another checkout state | **not recorded** (this issue, fixed for manifests and board descriptors) |

Configure-path reads outside the project were found with:

```
git grep -nE 'fs::read_to_string\(.*join\("(Cargo|nros-board)\.toml"\)' -- packages/cli/nros-cli-core/src
```

and each hit classified as inside the project (already in the digest) or
outside it. The sweep also found the SDK index and the platform
`nros-platform.toml` files, which are loaded in other crates; they stay
unrecorded and are tracked as issue 2485.

## Fix

- `configure_stamp::read_external(path)` is the one spelling for a
  configure-path read of a file that may lie outside the project. It records
  the canonical path and the digest of the bytes read. `configure` clears the
  record before C1 (`begin_external_reads`), and `stamp_now` stores the reads
  outside the project in `Stamp::external`. `currency` re-reads each and calls
  the stamp stale when one changed or is gone.
- Routed through it: `facade::crate_declares_feature`,
  `facade::crate_default_features`, `facade::board_feature_passthrough`,
  `backend_link::board_links`, the board-crate feature table in
  `build.rs`'s backend derivation, and both descriptor reads in
  `BoardCatalog`.
- `STAMP_FORMAT` 3 → 4, so a stamp written before the field existed is stale
  once, which is what clears a checkout already holding the stale table.

Regression test: `configure_stamp::tests::an_external_read_is_a_stamp_input`
(a feature added to an outside manifest, then the manifest removed). With
the check removed it fails.
