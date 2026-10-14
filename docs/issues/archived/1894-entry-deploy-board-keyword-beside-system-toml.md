---
id: 1894
title: "`nano_ros_add_executable(DEPLOY|BOARD …)` wins over the leaf `system.toml` with no comparison"
status: resolved
resolved: 2026-10-15
type: tech-debt
area: [cmake, build]
severity: low
found: 2026-10-14
related: [0934, RFC-0103, phase-484]
---

Split out of [issue 0934](0934-config-redundancy-map.md) (R13) when
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

## Fix (2026-10-15)

Measured first, as the direction asked. Over every tracked `CMakeLists.txt` and
`*.cmake`, a call to `nano_ros_add_executable` / `nano_ros_entry` names
`DEPLOY` or `BOARD` in exactly three places, and none of them sits beside a
leaf `system.toml`:

| caller | keywords | beside a `system.toml`? |
| --- | --- | --- |
| `cmake/NanoRosVerbs.cmake` (`nano_ros_add_executable` → `nano_ros_entry`) | the RESOLVED values | forwards, so not a statement |
| `fixtures/multi_pkg_workspace_cpp/src/demo_entry` | `DEPLOY linux` | no |
| `fixtures/cmake_node_register_misuse/cases/cmake_entry_rejects_embedded_deploy.cmake` | `DEPLOY linux zephyr` | no (a refusal case) |

The generated workspace root (`builder/cmake_root.rs`) also emits `BOARD` and
`DEPLOY`, taken from the resolved image it was generated for, and its
directory holds no `system.toml`. So there was no agreeing caller to drop, and
the change is the comparison alone:

- `cmake/NanoRosEntryAgreement.cmake` — `nros_entry_keywords_agree(<who>
  [DEPLOY …] [BOARD …])`. With no `system.toml` read (`NROS_EXPORT_FOUND`
  false) the keyword stands. With one, a `DEPLOY` other than the image's deploy
  token, or a `BOARD` other than its board (by alias or cmake module; a board
  that IS its deploy token matches that token), is a `FATAL_ERROR` naming the
  call and the `system.toml` path, which the reader now exports as
  `NROS_EXPORT_SYSTEM_TOML`.
- `nano_ros_add_executable` checks what ITS caller wrote, before defaulting.
  `nano_ros_entry` checks only when called directly: from
  `nano_ros_add_executable` it receives resolved values, including a
  `-DNROS_BOARD` front-end override, which must not read as a disagreement.
- Gate `cmake-entry-agreement` (`tests/cmake-entry-agreement-tests.sh`, eight
  assertions in a real `project()`); removing the DEPLOY refusal fails two of
  them.
- Measured end to end on `examples/linux/c/talker`: unchanged, it configures;
  with `BOARD stm32f4` added to its call the configure stops naming
  `talker/system.toml` and `[image.linux]`'s board `linux`; with `BOARD linux
  DEPLOY linux` it configures.
