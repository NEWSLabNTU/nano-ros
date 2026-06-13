---
id: 51
title: Deploy-target SSOT split — Phase-172 root `nros.toml` path contradicts RFC-0004 (`nros new --deploy` writes a file the loader rejects)
status: open
type: tech-debt
area: cli
related: [rfc-0004, phase-227, phase-211]
---

## The contradiction

RFC-0004 §4 (Stable, design-of-record) makes **`system.toml`** the home for
`[deploy.<id>]` targets + the system overlay (rmw / domain_id / lifecycle /
params), and **rejects a workspace-root `nros.toml`**. `nros.toml` survives only
as the embedded single-node direct-mode runtime file the board parses at boot.

The main config loader already enforces this:
`nros_config::NrosConfig::from_cargo_metadata` returns
`NrosConfigError::NrosTomlNotSupported` for a workspace-root `nros.toml`
(`packages/cli/nros-cli-core/src/orchestration/nros_config.rs:210`, test
`rejects_root_nros_toml`). Deploy targets on the live path come from
`[package.metadata.nros.deploy.<t>]` (Cargo metadata) / `system.toml`
`[deploy.<t>]` → `DeployTarget` rows.

But a **separate Phase-172 WP-A subsystem still reads/writes root `nros.toml`**:

- `orchestration/root_config.rs` — `WorkspaceConfig` (`[workspace]` /
  `[system]` / `[systems.<name>]` / `[deploy.<name>]`) loaded from root
  `nros.toml`.
- `cmd/scaffold_deploy.rs` — `nros new --deploy <name>` **appends
  `[deploy.<name>]` to the root `nros.toml`** and bails if none exists.
- `cmd/check.rs:112` — `nros check <root nros.toml>` validates via
  `WorkspaceConfig::load`.
- `cmd/doctor.rs` — `--config` defaults to `nros.toml`; checks deploy-target
  vendor pins via `WorkspaceConfig::load`.
- `book/src/reference/cli.md:74` documents `nros new --deploy` as scaffolding
  into the **root `nros.toml`** — contradicting the same book's RFC-0004
  `deployment.md` (updated in phase-227.7).

## Impact

`nros new --deploy` is effectively **unusable**: it requires/creates a
workspace-root `nros.toml`, which every other CLI verb (`plan`, `check` on a
real workspace, `codegen[-system]`, `metadata`) rejects with
`NrosTomlNotSupported`. The deploy-target SSOT is split across two files, one of
which the design forbids. Phase-227 converged the loader, examples, and book
config pages onto RFC-0004 but did not migrate this `scaffold_deploy` /
`root_config` / `check` / `doctor` cluster.

This also blocks the optional 211.F item ("map `host_id` partitions onto
`system.toml` `[deploy.<id>]` targets via `scaffold_deploy`"): the scaffolder
writes the wrong file.

## Fix direction (RFC-0004 is locked — convergence, not redesign)

Retire the root-`nros.toml` deploy path and point the scaffolder + validators at
the RFC-0004 home:

1. `nros new --deploy <name>` writes `[deploy.<name>]` into the bringup pkg's
   `system.toml` (or `[package.metadata.nros.deploy.<name>]` Cargo metadata) —
   NOT root `nros.toml`.
2. `nros check` / `nros doctor` read deploy targets through `nros_config` (the
   live loader) instead of `WorkspaceConfig::load`.
3. Retire `root_config::WorkspaceConfig` + the root-`nros.toml` branches once
   (2) lands.
4. Fix `book/src/reference/cli.md:74`.

Open question for the implementer: which `system.toml` does `nros new --deploy`
target when a workspace has multiple bringup pkgs, and does it create one if
absent? (Resolve before coding step 1.)
