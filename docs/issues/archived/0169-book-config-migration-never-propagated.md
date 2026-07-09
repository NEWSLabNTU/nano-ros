---
id: 169
title: "Book still documents retired per-example nros.toml/config.toml — phase-248/256 config migration never propagated (15 pages, 404 links)"
status: resolved
type: tech-debt
area: docs
related: [rfc-0004, phase-256, phase-277]
resolved_in: "2026-07-09 book config-migration sweep"
---

## Problem (summary)

The phase-248/256 config migration (per-example `nros.toml`/old
`config.toml` → `[package.metadata.nros.deploy.<t>]` + `nano_ros_deploy()` +
`system.toml`) never reached the book: 15 pages — several freshly dated by
the phase-277 refresh — still showed `nros.toml` file trees, verbatim blocks,
and GitHub links to files that don't exist (`find examples -name nros.toml`
returns zero). RFC-0004 §9 itself listed "Book sync: configuration.md still
documents the Phase 172.K model" as a known gap.

## Resolution

Ground truth taken from RFC-0004 (Stable, the design-of-record) + the shipped
examples' actual manifests. Changes:

- **`user-guide/configuration.md` rewritten** around the live model: one-home
  -per-concern table, precedence ladder, embedded `deploy` config (Rust
  metadata → `DeployOverlay`; CMake `nano_ros_deploy`), the **supported**
  standalone direct-mode `config.toml` for hand-written `no_std` apps
  (issue 0081 wontfix nuance — only the old `[network]/[zenoh]/[scheduling]`
  schema is retired), and an explicit "Retired files" section. Env-var /
  size-knob / footprint sections kept verbatim.
- **Embedded starters** re-grounded on real shipped shapes: `freertos.md`,
  `threadx.md`, `bare-metal.md`, `esp32.md`, `integration-nuttx.md`,
  `integration-esp-idf.md`, `integration-zephyr.md` — file trees, verbatim
  config blocks (now quoting the actual `Cargo.toml` deploy tables /
  `CMakeLists.txt` `nano_ros_deploy` calls), 404 links removed, and the
  fixture-port vs copy-out-port distinction stated where the two differ
  (fixtures bake per-language ports; shipped examples dial the default).
- **First-node pages**: `first-node-rust.md` dropped the retired
  `config.toml [zenoh]` block (env-only on native); `first-node-c.md`
  dropped the nonexistent `nros_config_load()` + optional-sidecar claims;
  `first-node-cpp.md` embedded-variant claim corrected to `nano_ros_deploy`.
- **`troubleshooting-first-10-min.md`** locator-fallback chain corrected
  (env → build default on native; compile-baked on embedded).
- **`porting-a-cpp-node.md`** parameter bullet rewritten to the RFC-0004 §10
  baked-initials + volatile-store model (dropped nonexistent
  `nano_ros_read_config`/`nros bake-params`).
- **`setup-compared-to-ros2.md`**, **`workflow.md`**,
  **`creating-examples.md`**, **`message-generation.md`** (canonical
  `--generate-config` spelling; `nros sync` promoted as the one-shot
  codegen+patch flow, `generate-rust` documented as the side-effect-free
  primitive) — all aligned.

Kept as-is (legitimately reference the retired surface): `reference/cli.md`
(documents the real `--nros-toml` overlay flag, `component_nros.toml`, and
the root-`nros.toml` rejection/lint machinery) and
`reference/nros-bridge-toml.md` (name-collision note). `concepts/`,
`porting/vendor-overlay.md` `Config::from_toml(include_str!("../config.toml"))`
snippets are the supported direct-mode path.

Verified: `mdbook build` green; `grep -rn nros.toml book/src` reduced to the
cli.md CLI-surface mentions + configuration.md's own retirement notes.
