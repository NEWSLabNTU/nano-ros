---
id: 2637
title: "nros dev image board-facts loaded the checkout's board catalog alone, so a workspace board package nros build found failed in the west configure"
status: resolved
type: bug
severity: medium
area: [build, zephyr]
related: [phase-478, 2157]
found: 2026-11-16
resolved: 2026-11-16
resolved_in: "phase-478 I23"
---

GitHub: NEWSLabNTU/nano-ros#4025

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)
I23** (island and consumer findings). Found by ASI's phase-15 migration.

## What happened

Two board lookups disagreed. `nros build` resolves an image's board against
the checkout's boards, `$NROS_EXTRA_BOARD_PATH`, AND the board packages the
workspace carries (`BoardCatalog::load_with_packages`, since issue 2157's
"a board is a package"). `nros dev image board-facts`, which
`cmake/NanoRosBoardFacts.cmake` runs inside the west configure `nros build`
spawns, loaded `BoardCatalog::load(root)`: the checkout and
`$NROS_EXTRA_BOARD_PATH` only. So a workspace board resolved for the build and
then failed in its own configure:

    nros dev image board-facts $PWD/src/controller_bringup --deploy s32z

failed in ASI's `actuation_module` (board `s32z270-rtu0-zephyr` from
`src/s32z270_zephyr_board`) and worked with
`NROS_EXTRA_BOARD_PATH=src/s32z270_zephyr_board`, which is why ASI's
`build.sh` exports it.

`leaf_system::load_catalog` had a third copy of the workspace search.

## Fix

`cmd::board::catalog_for_dir(root, dir)`: the workspace `dir` sits in
(`image_facts::workspace_root_from`), then the same `workspace_catalog`
`nros board` / `nros check` use, which is `nros build`'s `load_with_packages`
over the discovered packages. `board_facts` loads it once per resolve and
passes it to `pick_deploys`, `resolve_one` and the cargo-leaf lookup, and
`leaf_system::load_catalog` calls it instead of repeating the search.

## Test

`board_facts::tests::a_workspace_board_package_resolves_here_as_it_does_for_nros_build`:
a temp workspace with `src/my_board` (descriptor + `package.xml`) and a
bringup naming it. `nros build`'s catalog finds it, the checkout's alone does
not, `catalog_for_dir` finds it from the bringup and from the root, and
`resolve` answers `NROS_BOARD=my-ws-board`, with no `$NROS_EXTRA_BOARD_PATH`.

## Consumer cleanup

ASI drops the `NROS_EXTRA_BOARD_PATH` export in `build.sh:427-432` (it is also
moot once ASI uses nano-ros's own `s32z270-rtu0-zephyr`, issue 2631).
