---
id: 1894
title: "`nano_ros_add_executable(DEPLOY|BOARD …)` wins over the leaf `system.toml` with no comparison"
status: open
type: tech-debt
area: [cmake, build]
severity: low
found: 2026-10-14
related: [0934, RFC-0103, phase-484]
---

Split out of [issue 0934](archived/0934-config-redundancy-map.md) (R13) when
that survey was closed.

## What is true on main (2026-10-14)

An entry's deployment has one AUTHORED home now: the leaf's or bringup's
`system.toml` `[image.<id>]`. `find_package(nano_ros)` reads it into
`NROS_DEPLOY` / `NROS_BOARD` (`nano_rosConfig.cmake`). The retired spellings are
refused: `[package.metadata.nros.entry] deploy` (`check_workspace.rs`), the
`package.xml <nano_ros …/>` tuple (`NanoRosPackageXml.cmake`) and
`[deploy.*]` (the schema).

One spelling is still accepted beside it. `nano_ros_add_executable` /
`nano_ros_entry` take `DEPLOY` and `BOARD` keywords
(`cmake/NanoRosVerbs.cmake`, the `cmake_parse_arguments` near :139). The
keyword wins when given, and the `system.toml`-derived value is used only when
it is absent ("an explicit keyword still wins"). Nothing compares the two, so a
leaf whose `system.toml` says one board and whose `CMakeLists.txt` says
another builds the CMakeLists one and says nothing. That is RFC-0103's "one
fact, two owners, the quiet one wins".

## Not this

The many `DEPLOY linux` / `DEPLOY nuttx zephyr` lines under
`examples/templates/` and `packages/testing/nros-tests/fixtures/` belong to
`nano_ros_node_register` / `nros_components_register_node`. There `DEPLOY` is
a LIST of the deploys a COMPONENT supports, a different fact from which
deploy an ENTRY image is. Do not sweep them together.

## Direction

When a leaf `system.toml` was read (`NROS_EXPORT_FOUND`), an explicit entry
`DEPLOY`/`BOARD` that DIFFERS from the derived value is a configure error
naming both places; one that agrees is redundant and can be dropped from the
callers (then the keyword survives only for guest projects with no
`system.toml`). Measure the callers first: `git grep -n 'nano_ros_add_executable\|nano_ros_entry'`
in tracked `CMakeLists.txt`, keyword by keyword.
