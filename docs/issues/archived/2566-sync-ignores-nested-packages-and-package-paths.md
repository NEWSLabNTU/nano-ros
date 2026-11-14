---
id: 2566
title: "`nros configure` (reported as `nros sync`) finds packages only one level under `src/`: `src/<group>/<pkg>` and `nros.toml` `package_paths` are ignored"
status: resolved
type: bug
area: [cli]
severity: medium
found: 2026-11-14
resolved: 2026-11-15
resolved_in: "the change that resolved it (the scan and its tests, one change)"
related: [phase-478, rfc-0023]
---

GitHub: NEWSLabNTU/nano-ros#3858

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)**.
Seen on nano-ros 0.6.0-nros8.

## What happens

`nros configure` (reported as `nros sync`) only finds packages exactly one level under `src/`. Packages at
`src/<group>/<pkg>` are ignored, and so are the paths in `nros.toml`
`package_paths`.

Autoware workspaces group packages (`src/<group>/<pkg>`), so ported trees have
to be flattened today.

## Direction

Scan recursively, stopping at a `package.xml` and skipping `COLCON_IGNORE`
(colcon's rule), and honour `package_paths`.

## Resolution

`scan_workspace` (`packages/cli/nros-cli-core/src/cmd/ws.rs`), which feeds
both `nros configure` and `nros sync`, now walks with the provider scan's
recursive walk, `provider_scan::scan_workspace_packages`. That is colcon's
rule: recurse, stop at a `package.xml`, and prune build output and any
directory carrying an ignore marker. The walk covers `src/` and every
`nros.toml` `[workspace] package_paths` entry, taken from the same
`source_roots` list `nros build` reads. A `package_paths` entry that is not a
directory is a warning. The same package name in two directories is refused,
naming both. `has_pkg_subdir`, which decides that a workspace is a colcon
layout at all, asks the same walk, so a `src/` holding only
`src/<group>/<pkg>` packages is one too.

Tests (`scan_workspace_tests`):
- grouped packages are found, and a package is not entered;
- `package_paths` entries are scanned;
- one name in two directories is refused;
- a grouped `src/` is a colcon layout.

No tracked workspace in this tree gains a package from the change: no tracked
`package.xml` sits below `src/<x>/` outside another package.
