---
id: 2566
title: "`nros sync` finds packages only one level under `src/`: `src/<group>/<pkg>` and `nros.toml` `package_paths` are ignored"
status: open
type: bug
area: [cli]
severity: medium
found: 2026-11-14
related: [phase-478, rfc-0023]
---

GitHub: NEWSLabNTU/nano-ros#3858

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)**.
Seen on nano-ros 0.6.0-nros8.

## What happens

`nros sync` only finds packages exactly one level under `src/`. Packages at
`src/<group>/<pkg>` are ignored, and so are the paths in `nros.toml`
`package_paths`.

Autoware workspaces group packages (`src/<group>/<pkg>`), so ported trees have
to be flattened today.

## Direction

Scan recursively, stopping at a `package.xml` and skipping `COLCON_IGNORE`
(colcon's rule), and honour `package_paths`.
