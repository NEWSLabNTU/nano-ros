---
id: 2746
title: "A Zephyr package builds one program: a second own-`main` program fails to link, and a second registered component is refused"
status: open
type: limitation
severity: low
area: [zephyr, owned-entry, cmake]
related: [rfc-0071, phase-583, issue-2744]
found: 2026-11-20
---

## Measured

On a scratch copy of `examples/zephyr/c/talker` (Zephyr 3.7, native_sim),
with a second program added beside the registered component
(`nano_ros_add_executable(talker_b src/main_b.c)`, whose `main_b.c` defines
`main`), `nros build` failed at the link:

    zephyr_zenoh_entry_nros_main_generated.c:67: multiple definition of `main';
    app/libapp.a(app_app_main.c.obj): ...
    collect2: error: ld returned 1 exit status

The failure is loud, not a silent drop. A second registered component in a
package that names a DEPLOY is refused at configure since issue 2744.

## Why

A west build is ONE Zephyr image: `find_package(Zephyr)` creates one `app`
target, linked whole-archive into one `zephyr.elf`. Every nano-ros program
on this road is appended to it (`target_sources(app …)` in
`cmake/NanoRosEntry.cmake`, `_nros_is_zephyr` branch), so two programs are
two `main`s in one image. The other RTOS roads link each program as its own
executable (`add_executable` + `nros_platform_link_app`), which is why
RFC-0071 D20 gives them one firmware per executable and Zephyr does not
get it yet.

## Why this is a gap and not the design

D20 is one rule for native and RTOS: an owned entry may build several
programs, and the run chooses one. Zephyr is the road where it does not hold
yet. Zephyr can build several images in one build through **sysbuild**
(`ExternalZephyrProject_Add`, one image per child build dir). That is the
likely route: one child image per executable, each its own `app`, and the
run choosing which ELF to load.

## Until then

phase-583 W1 keeps the refusal on Zephyr, worded for the user: "Zephyr
builds one program per package; split the package".

## Next

Prototype one sysbuild child image per executable for a two-program
package on native_sim. Then measure that both ELFs build and boot, and what
sysbuild does to the board, flash and `nros run` roads, which today take one
west build dir.
