---
id: 2353
title: "`nros run` on mps2-an385-zephyr: the west runner does not find the workspace without ZEPHYR_BASE, and the plan starts no router for the board's 10.0.2.2 locator"
status: resolved
type: bug
severity: medium
area: [cli, runner, zephyr]
related: [phase-548, 0892, 2301, 2315]
found: 2026-11-07
resolved: 2026-11-09
resolved_in: "the run plan carries the build's ZEPHYR_BASE (from its CMake cache) as args.zephyr_base, which the west and FVP runners put in west's environment; the router half is issue 2301's fix"
---

**Home: [phase-548](../../roadmap/phase-548-three-languages-every-board.md)**
(found by W2; every language on mps2-an385 Zephyr meets it).

## What

Found while measuring phase-548 W2 in the `ros2` box (Humble, Zephyr 3.7).
The workspace came from `nros new <ws> --board mps2-an385-zephyr --template
talker --rmw zenoh`, and `nros build` succeeded. `nros run` then fails in two
independent ways, for C and C++ alike.

1. **The runner does not resolve the west workspace.** With `ZEPHYR_BASE`
   unset, `nros_runner_west` fails with `west: unknown command "build"`.
   `nros build` resolves the Zephyr workspace itself, through the same ladder
   `nros locate` uses, so the build works and the run does not. The runner is
   handed no workspace and runs `west` from a directory west does not
   recognise.
2. **The run plan starts no router.** The image states no `locator`, so the
   plan has no `link`. The board's own configuration points the guest at
   `tcp/10.0.2.2:7447`, which is the host as seen from QEMU's user-mode
   network, but the plan does not read it, so `--router auto` starts nothing
   and the talker has no router to reach.

## The workaround this retires

`book/src/starters/mps2-an385-zephyr.md`, "Run it in QEMU", has the reader
start `rmw_zenohd` by hand, listening on `tcp/0.0.0.0:7447`, and export
`ZEPHYR_BASE`. The router block carries the comment
`<!-- removed when the board descriptor states its default locator -->`.
Closing this issue removes that block and that note, and the probe track
replays the chapter with a plain `nros run`.

## Fix direction

- The west runner takes the workspace from the plan (the same answer
  `nros build` used), never from the caller's environment alone.
- The board descriptor states its default locator (`tcp/10.0.2.2:7447`
  from the guest, so the host router listens on `0.0.0.0:7447`). The run
  plan derives the router's listen endpoint from it, the same way it does for
  a board whose locator is a host loopback address.
- Coordinate with the planned change that makes `--router auto` reuse a
  router already listening (the user's ruling of 2026-11-07, not yet filed),
  since the chapter's router step and `nros run` meet there too.

## Done when

On mps2-an385-zephyr, in a fresh shell with no `ZEPHYR_BASE`, `nros run` in
a scaffolded workspace starts a router, boots the image and prints
`Publishing:`, and the book's hand-started router block is gone.

## Resolution (2026-11-09)

The two halves were different bugs. The second was issue 2301.

**1. The west workspace.** Measured in the `ros2` box with `ZEPHYR_BASE`
unset: `west build --help` works from inside a west workspace (the store's
`$NROS_HOME/workspaces/zephyr/3.7`), and anywhere when `ZEPHYR_BASE` names
the Zephyr tree. From a directory outside any west workspace it fails with
`west: unknown command "build"`. `nros build` passes west the Zephyr it
resolved, through `build::zephyr_base`'s ladder. The runner ran `west` with
the caller's environment, from the workspace directory. So the run failed
whenever the `nros new` workspace sat outside the west workspace and the
shell had no `ZEPHYR_BASE`. That explains why phase-548 W7 did not see it on
the Nucleo: there the workspace sits inside the west workspace.

Fix: the plan carries the Zephyr tree the build was configured against.
`run_plan::west_build_zephyr_base` reads `ZEPHYR_BASE:PATH` from the west
build's `CMakeCache.txt`. That is the build's own record, so it is the same
answer `nros build` used, including a `--zephyr-workspace` that `nros run`
does not repeat. `finish` puts it in `args.zephyr_base`, on a new `build`
provenance rung. `nros_runner_west::west_command` puts it in west's
environment, above an inherited `ZEPHYR_BASE`. Both the `west` runner (run
and flash) and the `fvp` runner, which runs the same `west build -t`, use
it. A plan that carries none leaves west's own lookup as it was.

**2. No router.** This is [issue 2301](2301-zephyr-run-plan-cannot-see-kconfig-locator.md),
resolved in the same change: the plan's `board` rung reads the locator the
image's build baked from its `.config`.

The book's hand-started router block in `starters/mps2-an385-zephyr.md` is
gone. That page still exports `ZEPHYR_BASE` while it sets up its own west
workspace (`zephyr_ws`). `nros build` needs that export to find a workspace
that is not on its ladder (`--zephyr-workspace`, `ZEPHYR_BASE`,
`NROS_ZEPHYR_WORKSPACE`, the store, or the checkout trees). `nros run` no
longer does.

## Verified

- `cargo test -p nros_runner_west`:
  `west_takes_the_zephyr_tree_from_the_plan` checks that a plan's
  `args.zephyr_base` becomes west's `ZEPHYR_BASE` and that a plan with none
  sets nothing. `cargo test -p nros-cli-core --lib`:
  `a_zephyr_image_links_to_the_locator_its_build_baked` reads
  `args.zephyr_base` from a fixture build's CMake cache.
- In the `ros2` box (Humble, Zephyr 3.7), 2026-11-09, with `ZEPHYR_BASE`
  unset, in a scaffolded workspace outside any west workspace:
  - Before: `nros run zm_node --expect 'Publishing:'` on mps2-an385-zephyr
    printed `west: unknown command "build"; do you need to run this inside a
    workspace?` and exited after 0.2 s. Its plan had no `[link]`.
  - After: the plan carries `zephyr_base =
    "/home/aeon/.nros-box/workspaces/zephyr/3.7/zephyr"` and
    `link = tcp/10.0.2.2:7447`. The run printed `nros run: starting the
    zenoh router on tcp/127.0.0.1:7447`, `-- west build: running target
    run_qemu` and `nros run: matched after 4.3 s: Publishing: 'Hello Wo`.
    `ros2 topic echo /chatter std_msgs/msg/String --qos-reliability
    best_effort` received `data: 'Hello World: 1'`. A router started by hand
    on `0.0.0.0:7447` was reused.
  - native_sim (`--board zephyr`), same shell: `nros run zn_node --expect
    "Publishing: 'Hello World: 25"` printed `nros run: starting the zenoh
    router on tcp/127.0.0.1:7447` and matched after 25.3 s, and the echo
    received `data: 'Hello World: 15'`. native_sim runs as a host process,
    not through west, so it never needed the workspace.
