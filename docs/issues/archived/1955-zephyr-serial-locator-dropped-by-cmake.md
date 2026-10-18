---
id: 1955
title: "A Zephyr serial image baked an empty locator: CMake drops a definition containing `#`"
status: resolved
type: bug
area: [zephyr, cmake, rmw]
severity: high
found: 2026-10-18
related: [1299]
---

## Measured

The safety island's S32K344 image (zenoh over LPUART,
`CONFIG_NROS_ZENOH_LOCATOR="serial/uart@40330000#baudrate=921600"`), rebuilt
on nano-ros main `3a72b3f23b`, never joined: the boot record stopped at stage
2 (`BootConfigResolved`), `LAST ERROR Transport ConnectionFailed`,
`nros_cpp_init` returned -100. The gateway was up on the serial line and saw
nothing. The image held no copy of the locator string (`strings`), and no
compile command in `build.ninja` carried `NROS_ENTRY_LOCATOR`. The configure
log said why, once per target:

    WARNING: Preprocessor definitions containing '#' may not be passed on the
    compiler command line because many compilers do not support it.
    CMake is dropping a preprocessor definition:
    NROS_ENTRY_LOCATOR="serial/uart@40330000#baudrate=921600"

Bisected on the board over d4bf898409..3a72b3f23b: `4f8f9a73f1` boots to
FirstSpin, `2ccf662c0d` (issue 1299, RFC-0071 D10) does not; the commits
between are a test and two docs.

## Cause

2ccf662c0d moved the zenoh locator from `entry_config.h`, which read
`CONFIG_NROS_ZENOH_LOCATOR` from `autoconf.h`, to
`zephyr/cmake/nros_rmw_zenoh.cmake`, which passes it as
`zephyr_compile_definitions("NROS_ENTRY_LOCATOR=\"...\"")`. CMake refuses a
`#` in a definition on the command line and drops it with only a warning, so
`entry_config.h`'s empty bottom rung won. Every serial locator has a `#`
(`serial/<dev>#baudrate=<n>`); tcp/udp locators do not, which is why the
Zephyr e2e cells (tcp) stayed green.

## Fix

`nros_rmw_zenoh.cmake` writes the `#` as the C octal escape `\043`; the
string literal the compiler builds is the locator unchanged (checked with a
CMake/Ninja probe printing it, and on the board: the image carries the
locator again and boots to FirstSpin).
