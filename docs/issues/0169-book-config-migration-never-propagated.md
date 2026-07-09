---
id: 169
title: "Book still documents retired per-example nros.toml/config.toml — phase-248/256 config migration never propagated (15 pages, 404 links)"
status: open
type: tech-debt
area: docs
related: [rfc-0004, phase-256, phase-277]
---

## Problem

The phase-248/256 config migration (per-example `nros.toml`/`config.toml` →
`[package.metadata.nros.deploy.<target>]` in `Cargo.toml`,
`nano_ros_deploy(... DOMAIN_ID ... LOCATOR ...)` in CMake, plus `system.toml`)
was **never propagated into the book**. The phase-277 refresh (2026-07-03)
updated example prose and links but preserved the obsolete config-file shapes —
pages carry fresh dates with stale content.

Evidence (verified 2026-07-09):

- `find examples -name nros.toml` returns **zero** files; 79 example
  `Cargo.toml`s use the deploy-metadata form (e.g.
  `examples/threadx-linux/rust/talker/Cargo.toml:42`), C/C++ use
  `nano_ros_deploy()` (e.g. `examples/qemu-arm-freertos/c/talker/CMakeLists.txt:48`).
- 15 book pages contain literal `nros.toml`; every **per-example file path** is
  a dead link:
  - `getting-started/freertos.md:46,54,59,73,75,108` — file-tree + GitHub link
    to `examples/qemu-arm-freertos/rust/talker/nros.toml` (404).
  - `getting-started/threadx.md:86,105` — two 404 links.
  - `getting-started/first-node-rust.md:66,128-142` — shows a runtime
    `config.toml` `[zenoh]` block while `reference/cli.md:148-150` states
    `config.toml` is retired (phase-256, RFC-0004 §8). Internal contradiction.
  - Also: `first-node-c.md`, `esp32.md`, `bare-metal.md`,
    `integration-nuttx.md`, `integration-esp-idf.md`, `integration-zephyr.md`,
    `porting-a-cpp-node.md`, `troubleshooting-first-10-min.md`,
    `setup-compared-to-ros2.md`.
- `user-guide/configuration.md:18,44` — the whole page is built around the
  retired file model. **Needs rewrite**, not touch-up.
- `user-guide/message-generation.md:120-135` — documents a git-dep
  (`nros = { git = … }` + `--nano-ros-git`) flow that contradicts the current
  `version = "*"` + `nros sync` `[patch.crates-io]` model documented in
  `installation.md`/`first-node-rust.md`. Also `:161` uses the `--config`
  alias where the canonical flag is `--generate-config`.

Note: a *workspace-root* `nros.toml` concept still exists in the CLI
(`cli.md:112,119`) — not every mention is wrong, only the per-example file
shapes and paths.

## Fix direction

One focused sweep: write the two canonical config-shape snippets (Rust
Cargo-metadata, C/C++ `nano_ros_deploy`) once, then replace the `nros.toml`
file trees / links / prose across the 15 pages. Rewrite
`user-guide/configuration.md` around the current model; reconcile
`message-generation.md` with the `nros sync` flow.
