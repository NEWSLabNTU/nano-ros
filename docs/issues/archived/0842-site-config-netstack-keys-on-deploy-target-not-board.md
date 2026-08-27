---
id: 842
title: "A migrated image inherited the DEFAULT rmw, and zenoh-pico's FreeRTOS
  backend is lwIP-only — the netstack was never involved"
status: resolved
type: bug
area: cmake
related: [rfc-0065, phase-383]
resolved_in: 70c979037
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

## Root cause — NOT what this issue first said

The netstack was innocent, and one command showed it:

```
nros ws board-facts --board freertos-posix --deploy freertos       -> NROS_NETSTACK=lwip
nros ws board-facts --board freertos-posix --deploy freertos-posix -> (none)
```

The deploy reaching that resolver was already `freertos-posix`, and `NROS_DEPLOY`
is unset in the PRE-migration cache too — so the two builds agreed about the
netstack all along.

What actually differed: the pre-migration fixture row built
`rmw = "cyclonedds"`, and the migrated `[image.freertos_posix]` inherited
`[image_defaults] rmw = "zenoh"`. zenoh-pico's FreeRTOS backend is lwIP-only
(`system/freertos/lwip/network.c`), and that board has no lwIP, so a zenoh build
compiles that TU and cannot link it.

The symptom named lwIP, so I reasoned about what could select an lwIP backend.
Reading the row being replaced would have been faster:

```toml
id = "workspace-c-freertos-posix"
rmw = "cyclonedds"        # <- the whole answer
```

## Fix

`rmw = "cyclonedds"` on the image. The RMW is a property of the image, which is
what RFC-0065 D6 says an image is for.

Three further defects fell out of finishing the migration, all fixed in the same
commit: dropped row `cmake_defs` (the `NROS_ENTRY_LOCATOR` a QEMU peer needs),
the `NUTTX_DIR` promotion that W4.a documents and never emitted, and two
platform vocabularies (`threadx-linux` from the board catalog vs `threadx_linux`
in `nros_feature_set` — emitted in one place now, because fixing only the
`set()` left `nano_ros_workspace(PLATFORM …)` failing from a different line).

`examples/workspaces/c` is fully migrated: root and all eleven derivable entry
packages deleted, thirteen rows on images, `zephyr_entry` hand-written by design.

## What the original directions got right

Nothing. Both proposed moving the netstack out of `[deploy.<target>.nros]` —
re-keying a config that was answering correctly. Recorded here because the next
reader of a wrong diagnosis deserves to know it was wrong before they act on it.

## Sweep

```sh
grep -rn 'netstack' examples/workspaces/*/src/*/system.toml packages/boards/*/nros-board.toml cmake/
grep -rln 'deploy\..*\.nros' examples/workspaces/*/src/*/system.toml
```
