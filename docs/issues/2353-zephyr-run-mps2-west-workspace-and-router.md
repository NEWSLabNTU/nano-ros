---
id: 2353
title: "`nros run` on mps2-an385-zephyr: the west runner does not find the workspace without ZEPHYR_BASE, and the plan starts no router for the board's 10.0.2.2 locator"
status: open
type: bug
severity: medium
area: [cli, runner, zephyr]
related: [phase-548, 0892]
found: 2026-11-07
---

**Home: [phase-548](../roadmap/phase-548-three-languages-every-board.md)**
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
