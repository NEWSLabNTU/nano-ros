---
id: 2342
title: "zenoh-pico has no FreeRTOS port for a host-owned network stack, so a zenoh image on the FreeRTOS POSIX board does not build"
status: open
type: bug
area: rmw-zenoh, freertos, boards
related: [issue-2341, issue-1143]
found: 2026-11-07
---

**Home: none yet.** Found while fixing issue 2341.

## Symptom

On `main` plus the 2341 fix, a scaffolded zenoh talker for the FreeRTOS POSIX
board (`nros new fp_ws --board freertos-posix --template talker --lang c`,
then `nros build`) gets `FREERTOS_PORT` to zenoh-pico's build, then stops:

```
lwip/opt.h: fatal error: lwipopts.h: No such file or directory
```

The same board with `--rmw cyclonedds` builds, and `nros run` prints
`Publishing: 'Hello World: 1'` onward.

## Cause

`packages/rmw/zenoh/nros-rmw-zenoh/nros-rmw.toml`, `[port.freertos]`, selects
`ZENOH_FREERTOS_LWIP` and the lwIP sources whenever the platform capability
`ip_stack = true`. The FreeRTOS POSIX board has no lwIP. Its network is the
host kernel's (`[board.net] stack = "host"`, `supported_netstacks = []`). So
zenoh-pico has no FreeRTOS arm whose sockets are the host's BSD sockets. Issue
1143 made the platform header a board fact for exactly this kind of split
(lwIP versus Orin SPE); a host-socket arm is the missing third case.

## Why it matters

The FreeRTOS POSIX starter (phase-537 W4) scaffolds the zenoh default, and its
probe track cannot replay. The board's registry evidence is the two
cyclonedds fixtures only, so nothing in CI builds a zenoh image for it.

## Options

1. **A host-socket FreeRTOS arm.** The board states its network stack, and
   `[port.freertos]` branches on it: lwIP boards keep `ZENOH_FREERTOS_LWIP`, and
   a `stack = "host"` board gets a zenoh-pico arm over the host's sockets (as
   the POSIX port's `unix` network layer, under FreeRTOS's scheduler).
2. **Refuse zenoh on the board,** naming cyclonedds. The scaffold for
   `freertos-posix` then defaults to cyclonedds, and the starter teaches that.

Until one lands, the book's FreeRTOS POSIX starter scaffolds `--rmw cyclonedds`.

## Done when

A scaffolded zenoh talker on `freertos-posix` builds and `nros run` prints
`Publishing:`, or zenoh on that board is refused at configure with a remedy,
and the scaffold no longer defaults to it.
