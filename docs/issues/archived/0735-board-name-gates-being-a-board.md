---
id: 735
title: "A board NAME is the precondition for being a board: `_nra_board_active` silently denies every integration-shell entry its glue, locator and app config"
status: resolved
type: bug
severity: high
area: cmake
related: [rfc-0064, phase-349, issue-0415]
---

# 0735 — `if(DEFINED NANO_ROS_BOARD)` is the wrong question

`cmake/NanoRosEntry.cmake:489`:

```cmake
    set(_nra_board_active FALSE)
    if(DEFINED NANO_ROS_BOARD)
        if(("${NANO_ROS_BOARD}" IN_LIST _NRA_DEPLOY)
           OR ("${NANO_ROS_PLATFORM}" IN_LIST _NRA_DEPLOY)
           OR ("${_nra_platform_norm}" IN_LIST _NRA_DEPLOY))
            set(_nra_board_active TRUE)
        endif()
    endif()
```

The outer guard makes a board **name** the precondition for asking whether the
entry is deployed to this build. That is a proxy for "this is an embedded
build", and it is exactly inverted for RFC-0064: a board arriving through an
integration shell **contributes no files to this tree and therefore has no
`NANO_ROS_BOARD` to define.** That is the design, not an oversight — the
FreeRTOS platform module's own board-less mode exists for it, and
`integrations/nano-ros` (ESP-IDF) is 146 lines with no board name at all.

So for every shell-integrated entry the predicate answers FALSE, and all three
dependent blocks are skipped silently:

| gated block | consequence |
| --- | --- |
| the `NROS_ENTRY_LOCATOR` + domain-id bake | the entry connects nowhere |
| **`nros_platform_link_app_deferred()`** | **no platform link at all** — no family C glue, no `freertos_platform` on the target |
| the FreeRTOS `NROS_APP_CONFIG` TU | network + task sizing silently fall back to defaults |

The middle row is the severe one. It is not a missing diagnostic: the configure
succeeds, the build succeeds, and the image is wrong.

## Blast radius

Every integration shell, not one board: ESP-IDF (`integrations/nano-ros`),
NuttX (`integrations/nuttx`), PlatformIO, and any vendor-SDK shell. All three
dependent blocks additionally require a non-posix platform, so the host lane is
unaffected — which is part of why this has stayed invisible.

## Fix

Keep the board-name test; stop letting it gate the platform tests:

```cmake
    set(_nra_board_active FALSE)
    if(("${NANO_ROS_BOARD}" IN_LIST _NRA_DEPLOY)
       OR ("${NANO_ROS_PLATFORM}" IN_LIST _NRA_DEPLOY)
       OR ("${_nra_platform_norm}" IN_LIST _NRA_DEPLOY))
        set(_nra_board_active TRUE)
    endif()
```

With `NANO_ROS_BOARD` unset the first clause is simply false and DEPLOY naming
the platform is what makes the entry active. Every in-tree board defines
`NANO_ROS_BOARD`, so their answer cannot change.

Verified locally on a 205a834-based tree: mps2 kept an identical TU set
(`board_mps2`, `freertos_c_entry`, `freertos_hooks`, `freertos_run_tiers`,
`net`, `network_glue`, `nros_app_config_def`), and a board-less entry went from
receiving **nothing** to receiving the glue and its app config.

## A note on how this was nearly missed

A first attempt at the surrounding work "validated" a board-less family-glue
target by calling `nros_platform_link_app` **by hand** in a probe. That passed
while the front door — `nano_ros_entry` — stayed shut, because this predicate
sits between them. A validation that reaches past the entry point is not a
validation of the entry point.

## RESOLVED (2026-08-20)

Outer guard dropped; the board-name test is now one of the three OR arms.

Verified through the FRONT DOOR — a real `nano_ros_entry` call, not a probe
that reaches past it (see the note above about how this was nearly missed):

| lane | before | after |
| --- | --- | --- |
| mps2 (`NANO_ROS_BOARD` set) | 7 TUs, `EXECUTABLE` | **identical** — `board_mps2`, `freertos_c_entry`, `freertos_hooks`, `freertos_run_tiers`, `net`, `network_glue`, `nros_app_config_def` |
| board-less (shell composed `freertos_platform`) | **nothing** | `nros_app_config_def.c` emitted and compiled into the entry |

**Necessary but not sufficient for a working board-less image.** This restores
the three blocks' *invocation*; the FreeRTOS family C glue is still not
compiled board-less, because `FREERTOS_STARTUP_SOURCE` is only ever populated
by a per-board overlay and `nros_platform_link_app` has nothing else to give.
That is a separate defect — the family owns the family C, and re-listing it per
board is what let board-less lose it. Tracked as the phase-351 work, not here.

## Suggested gate

An assertion that a non-posix, non-Zephyr entry ends up with a platform link:
after `nano_ros_entry`, `_NROS_PLATFORM_LINK_DONE` should be set on the target.
Cheap, and it fails loudly on exactly the silent case above.

Found bringing up the autoware-safety-island MRM chain via a vendor-SDK shell.
