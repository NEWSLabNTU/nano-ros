---
id: 2314
title: "A west-built Rust entry cannot find its bringup: `nros::main!`'s workspace walk stops at the generated entry's own `[workspace]` table"
status: resolved
type: bug
area: [zephyr, cli]
severity: high
found: 2026-11-06
resolved: 2026-11-07
related: [phase-531]
---

## What

The Zephyr Rust chapter (`book/src/guide/platforms/zephyr.md`, the
`zephyr-rust` probe track) scaffolds `nros new rs_ws --board zephyr --lang
rust` and runs `nros build`. The generated entry failed to compile:

    error: nros::main!: pkg `rs_node_bringup` not found in workspace
    `.../rs_ws/build/zephyr-zenoh/zephyr_entry`. Known pkgs: []

Reproduced on the base before phase-531 W5 (#3149), so W5 did not cause it.

## Why

`nros::main!` finds the bringup through `nros_pkg_index::detect_workspace_root`:
`$NROS_WORKSPACE_ROOT`, else a `.colcon_workspace` marker, else a `Cargo.toml`
with `[workspace]`, else `.git`. Every cargo road hands cargo
`NROS_WORKSPACE_ROOT` in its settings file (`builder::cargo_config`). The west
road cannot, because zephyr-lang-rust owns that cargo command line, and it
never got another carrier. In-tree workspaces track a `.colcon_workspace` and
sit under the repository's `.git`, so they resolved. A scaffolded workspace
outside a checkout has neither, so the walk stopped at the generated entry's
own `[workspace]` table (RFC-0098 D9), which holds no packages.

## Fix

- The west handoff passes `-DNROS_WORKSPACE_ROOT=<workspace>`
  (`builder::workspace_root_arg`), and `zephyr/CMakeLists.txt` appends it to
  `_nros_rust_lane_env`, the Rust lane's one carrier (issue 1662). It is a
  cached `-D`, so a later bare `west build` of the same build dir keeps it;
  it lives only in the build dir, so `nros materialize` copies nothing that
  names a path.
- `nros new <ws>` writes the `.colcon_workspace` marker, so a bare `west build`
  of a fresh build dir (no `nros build` in front) still resolves through the
  walk. A workspace scaffolded before this has no marker; there, only the
  `-D` names it.

A first version wrote a `relative = true` row into
`build/<coord>/<id>_entry/.cargo/config.toml` instead. It was dropped: the row
resolves against the directory holding `.cargo/`, so it held only while
cargo's cwd is the entry dir, and `nros materialize` would have copied it into
the user's tree with a wrong relative path.

Tests: `build_verb_pipeline::a_west_image_names_its_workspace_root`,
`new_workspace::tests::the_project_scaffold_writes_a_node_package_and_a_bringup`.
