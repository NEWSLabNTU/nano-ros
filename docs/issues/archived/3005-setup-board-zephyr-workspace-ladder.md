---
id: 3005
title: "`nros setup board` requires `--zephyr-workspace` where `nros build` finds the west workspace itself"
status: resolved
type: ux
severity: low
area: [cli, setup, zephyr]
found: 2026-11-27
resolved: 2026-11-27
resolved_in: "fix-3005-setup-board-ws-ladder"
reporter: "nros NXP 175 (while verifying the Zephyr SDK path fix, #4993)"
---

## What happened

Reported by NXP 175. In a project whose own west workspace is right there,
`nros setup board s32z270-rtu0-zephyr` needed `--zephyr-workspace ..`,
while `nros build` and `nros run` found the workspace on their own.

## Why

`BoardSetupArgs::zephyr_workspace`
(`packages/cli/nros-cli-core/src/cmd/setup.rs`) is a required argument,
with no default. `nros build` resolves the same place through a ladder
(`cmd/build.rs`, `zephyr_base_rung_with`):
1. `--zephyr-workspace`;
2. `$ZEPHYR_BASE`;
3. `$NROS_ZEPHYR_WORKSPACE`;
4. the project's own west workspace (`project_west_zephyr`, issue 2877);
5. the store's workspace, then the checkout-relative trees.

So the two verbs disagree about where the project's Zephyr workspace is.
`setup board` is the one a user runs first.

## What a fix looks like

- `setup board` takes the same ladder, with the flag optional, and says
  which rung answered. It writes into the workspace (it applies the patch
  set there), so it should take only the rungs that name the user's own
  tree: the flag, `$ZEPHYR_BASE`'s workspace, `$NROS_ZEPHYR_WORKSPACE` and
  the project's west workspace. Never the store's.
- With none of those, it refuses as today, naming the flag.
- A test: from inside a fake west workspace (`.west/` beside `zephyr/`),
  `setup board <name> --dry-run` names that workspace without the flag.

## Fix (2026-11-27)

`--zephyr-workspace` is optional. `setup board` resolves the workspace by
calling `nros build`'s own ladder (`build::zephyr_base_rung_with`), so the
two commands share one resolver and cannot drift:
1. `--zephyr-workspace`;
2. `$ZEPHYR_BASE`;
3. `$NROS_ZEPHYR_WORKSPACE`;
4. the west workspace the current directory is in.

The ladder's fifth rung, `Fallback` (the store's workspace, or a tree
beside the nano-ros checkout), is refused, because this verb writes the
patch set into what it resolves. A rung other than the flag is named on
stderr. With no rung the user's own, it refuses, naming the flag.

Test: `setup::tests::setup_board_takes_the_rung_nros_build_takes` runs five
environments (the flag, `$ZEPHYR_BASE`, `$NROS_ZEPHYR_WORKSPACE`, the
project's west workspace, a store only) through both commands, with a store
present. `setup board` lands on `nros build`'s tree, or refuses where
`nros build` falls back.

