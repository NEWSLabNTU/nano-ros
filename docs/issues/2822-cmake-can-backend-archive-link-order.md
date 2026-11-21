---
id: 2822
title: "cmake: the CAN backend archive is linked before component archives, so a component calling <nros/platform_can.h> fails to link"
status: open
type: bug
severity: high
area: [cmake, build, boards]
related: [phase-560, issue-2787, issue-2788, issue-2273]
found: 2026-11-22
---

**Home: [phase-560](../roadmap/phase-560-contract-revision-follow.md) W9**, beside
the census CAN mock (issues 2787/2788, PR #4577).

## What happens

`nros_entry_can_image` (`cmake/NanoRosCanImage.cmake`) links the CAN backend
archive into the entry with `target_link_libraries(<entry> PRIVATE
nros_can_census_mock)` (census host build) or `... nros_can_socketcan` (a host
image with `[image.<id>.can] device`). A component's archive reaches the entry
through its `<pkg>_<exec>_component` INTERFACE wrapper (the Phase 219.J
sidecar, `cmake/NanoRosEntry.cmake` ~L862). CMake emits the entry's direct
archive before the wrapper's closure, so ld's single pass has already passed
the backend when it meets the component that calls the driver.

The safety island's census host build of its controller fails:

    /usr/bin/ld: libcontroller_lib.a(controller_node.cpp.o): undefined reference to `nros_platform_can_open'

(also `close`, `send`, `fd_active`), then "could not take its census".

With the `socketcan` backend the same order is worse when the board staticlib
carries its own socketcan copy: the component's driver calls resolve there,
and that copy's device store is not the one the image's hand-off TU filled.

The same class as phase-544 W2 (issue 2273, `libkernel.a` on Zephyr).

## Repro (CMake 3.22)

    add_library(mock STATIC drv.c)
    add_library(comp_lib STATIC comp.c)          # comp() calls drv()
    add_library(comp_component INTERFACE)
    target_link_libraries(comp_component INTERFACE comp_lib)
    add_executable(entry main.c)                 # main() calls comp()
    target_link_libraries(entry PRIVATE comp_component)
    target_link_libraries(entry PRIVATE mock)

The link line is `cc main.o -o entry libmock.a libcomp_lib.a`:
`undefined reference to drv`.

`tests/can-census-mock-tests.sh` calls the driver only from the entry's own
source, so it never saw this.

## Exposure

- `nros_can_census_mock`: the census host build's entries, and the metadata
  probe's `nros_link_can_census_mock` (any archive the probe's component
  links in turn).
- `nros_can_socketcan`: a host image that names a CAN device.
- Not the fail-loud defaults (issue 2788): an object of the entry, so a later
  archive's references resolve against it.

## Wanted

Every CAN backend archive follows, on the link line, each archive the target
reaches, as W2 does for the kernel.

## Acceptance

A component archive reached through an INTERFACE wrapper, that alone calls the
driver, links against the census mock (census entry and probe roads) and the
socketcan backend; rows in `just check can-census-mock` that fail on main.
