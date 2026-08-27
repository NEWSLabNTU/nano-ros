---
id: 842
title: "A generated cmake root picks the wrong netstack for a board whose
  PLATFORM shares a `[deploy.<target>.nros]` block with another board"
status: open
type: bug
area: cmake
related: [rfc-0072, rfc-0065, phase-383, phase-351]
---

## Problem

`examples/workspaces/c` declares two FreeRTOS boards:

* `freertos` → `mps2-an385-freertos` (cross arm-none-eabi, lwIP)
* `freertos_posix` → `freertos-posix` (host cc, POSIX sockets)

Its site config is keyed on the DEPLOY TARGET, not the board:

```toml
[deploy.freertos.nros]
netstack = "lwip"
sdk = { freertos = "{env:FREERTOS_DIR}", lwip = "{env:LWIP_DIR}" }
```

A build of the `freertos_posix` image through the generated root resolves that
block and compiles zenoh-pico's `system/freertos/lwip/network.c` into
`libnros_cpp.a` — for a board that has no lwIP. It fails at LINK, as 63
`undefined reference to lwip_*`, from an archive that had no business containing
lwIP code:

```
libnros_cpp.a(…-network.o): in function `_z_open_udp_unicast':
  zenoh-pico/src/system/freertos/lwip/network.c:552: undefined reference to `lwip_socket'
```

The hand-written root did not have the problem, and the artifact proves it
rather than the configure: the pre-migration `freertos_posix_entry` binary
contains **zero** `lwip` strings.

## Why the hand-written root escaped it

It gated its `SUBDIRS` on `NANO_ROS_BOARD` and added exactly one entry per
board, so a `freertos-posix` configure was a *different configure* from a
`mps2-an385-freertos` one, with only that board's entry in it. The generated
root lists every discovered package and emits the entries for the coordinate, so
the board must be enough on its own to select the netstack — and it is not,
because the site config answers per deploy TARGET.

Ordering was ruled out on the way: `NANO_ROS_BOARD` / `NANO_ROS_PLATFORM` are
now emitted BEFORE `find_package(nano_ros)` (a real fix, kept — the
hand-written root receives them on the command line, i.e. before anything), and
the failure is unchanged.

## Scope

`phase-383` migrated this workspace's nine NATIVE rows; the four embedded ones
(`freertos`, `freertos_posix`, `nuttx`, `threadx`) still build through the
hand-written root, which was kept for exactly them. D13 is per-image by design —
delete a package and the next build emits its call — so a partially migrated
workspace is a supported state, not a half-finished one.

The same shape will block every remaining embedded image: `realtime-c`,
`realtime-cpp`, `mixed` all carry `[deploy.<target>.nros]` blocks.

## Directions

1. **Key the site config on the BOARD where a board is what varies.** RFC-0072
   §5 put it under `[deploy.<target>.nros]` when a deploy target and a board
   were effectively one thing. Two boards on one platform breaks that, and
   `[image.<id>]` is the natural home for what the image needs.
2. **Or resolve the netstack from the board descriptor** — it already declares
   `supported_netstacks` and `resolve_netstack`, so the fact exists; the
   generated root just does not consult it. The site config would then supply
   only the SDK paths, which are genuinely site facts.
3. Either way the generated root should pass the resolved netstack explicitly,
   the way it now passes `WORKSPACE_ROOT`: a generated file that depends on a
   lookup keyed differently than its own inputs is the seam this issue is.

## Sweep

```sh
grep -rn 'netstack' examples/workspaces/*/src/*/system.toml packages/boards/*/nros-board.toml cmake/
grep -rln 'deploy\..*\.nros' examples/workspaces/*/src/*/system.toml
```
