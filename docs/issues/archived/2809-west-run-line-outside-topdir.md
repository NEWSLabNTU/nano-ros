---
id: 2809
title: "The `west build -d <dir> -t run` line `nros build` prints fails outside a west workspace"
status: resolved
type: bug
severity: low
area: [cli, zephyr]
related: [issue-2808, issue-2176]
found: 2026-11-21
resolved: 2026-11-22
resolved_in: "fix(cli): `nros build`'s west run/flash lines work where printed (issue 2809)"
---

## What

After a Zephyr image builds, `nros build` prints its run target
(`cmd/build_next.rs`, `west_run`):

```text
  run on the FVP:
    west build -d <workspace>/build/zephyr-cyclonedds/demo_bringup__fvp_west -t run
```

Run as printed, from the image's workspace (where the user ran `nros build`),
west refuses it:

```text
west: unknown command "build"; do you need to run this inside a workspace?
```

`west build` is an extension command of the Zephyr west workspace, and the
image's workspace is not one. It works from inside the Zephyr workspace
(`~/.nros/workspaces/zephyr/3.7`), or with `ZEPHYR_BASE` exported to that
workspace's `zephyr/`. `nros build` found that workspace itself (through
`--zephyr-workspace`, `$ZEPHYR_BASE` or `$NROS_ZEPHYR_WORKSPACE`), so it knows
what the line needs and does not say it.

Measured 2026-11-21 on `examples/workspaces/realtime-cpp` `demo_bringup:fvp`
(FM 11.31.28): the printed line fails as above; the same line with
`ZEPHYR_BASE=~/.nros/workspaces/zephyr/3.7/zephyr` boots the image; and
`nros run demo_bringup:fvp` boots it with no extra setting.

The line is shared by every west board (`west_run`, `west_flash`), so the
other Zephyr emulator and flash lines likely have the same gap; only the FVP
one was measured.

## Direction

Print the line so it runs where the user is: prefix it with the
`ZEPHYR_BASE=<found workspace>/zephyr` it needs (or a `cd`), or print
`nros run <image>` first and the west line as the manual alternative. The
internals FVP page now names `nros run` and the `ZEPHYR_BASE` workaround
(issue 2808).

## Resolved, 2026-11-22

`nros build` now prints `nros run <image>` (or `nros flash <image>`) first,
since it works as printed, and the west line after it as the manual
alternative, prefixed with `ZEPHYR_BASE=<tree>/zephyr`. The tree is the one
the build itself resolved (`--zephyr-workspace`, `$ZEPHYR_BASE`,
`$NROS_ZEPHYR_WORKSPACE`, then the store workspace). When no tree was found,
the west line stays bare and a note says what it needs. Every west board
shares the change (`next_steps` in `cmd/build_next.rs`), so the emulator and
flash lines are covered as well as the FVP one. A Zephyr board with no
`[board.run]` keeps only the west line, because `nros run` cannot run it.

Held by the `build_next` tests: the `ZEPHYR_BASE` prefix on a found tree, the
bare line plus note with none, and the `nros run` / `nros flash` lines first.
