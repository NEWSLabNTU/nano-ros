---
id: 475
title: "The RMW static lib is an ORDER-ONLY link dep, so a backend change never relinks the C/C++ examples — museum binaries by construction"
status: resolved
type: bug
severity: high
area: cmake, testing
related: [issue-0196, issue-0391, issue-0445, issue-0466]
resolved_in: phase-209
---

> **Retitled 2026-08-07 after investigation.** This was filed as "the staleness
> probe is over-broad". That was WRONG, and the correction is the finding: the
> probe is right, and the build graph is under-specified. The original framing
> is kept at the bottom because the reasoning that produced it is the trap.

## Finding

`libnros_rmw_cyclonedds.a` is an **order-only** dependency of the example
binaries that link it:

```console
$ ninja -C examples/native/c/talker/build-cyclonedds -t query c_talker
c_talker:
  input: CXX_EXECUTABLE_LINKER__c_talker_Release
    CMakeFiles/c_talker.dir/src/main.c.o
    | libbuiltin_interfaces__nano_ros_c.a          <- implicit: change ⇒ relink
    …
    || nano_ros/…/libnros_rmw_cyclonedds.a          <- ORDER-ONLY: change ⇒ nothing
```

Order-only (`||`) means "must exist before linking"; a change to it never
triggers a relink. So **editing the CycloneDDS backend rebuilds the archive and
leaves every example binary containing the old backend code**, indefinitely.

Measured, on this tree:

```
libnros_rmw_cyclonedds.a   14:15:40   (rebuilt, correctly)
c_talker                   12:28:21   (older than its own link input)
$ cmake --build examples/native/c/talker/build-cyclonedds -j   # rc=0, no relink
```

Five objects that feed `c_talker` list `rmw_ret.h` among their deps, all rebuilt
at 14:15. The binary predates them by two hours and the build is "up to date".

## The probe was right

`cmake_dep_info_newer_source` reports STALE because a real dependency is newer
than the binary. That is exactly true. What made it look like a probe bug is
that the remedy it prints — "Run `just build-test-fixtures` first" — cannot
work: no build command can fix an edge the graph does not have. Only
`rm -rf <leaf>/build-<rmw>` does, at ~687 s per leaf (Cyclone self-provisions
from source), across ~8 leaves.

This is issue 0391's class (a fixture running as a museum binary because a real
input is invisible to the graph), except here the input IS in the graph — with
the wrong edge kind.

## Lead on the cause

`NROS_RMW_EXTRA_LINK_LIBS` is **set and never read**:

```console
$ git grep -rn NROS_RMW_EXTRA_LINK_LIBS
cmake/NanoRosRmwDispatch.cmake:24:  set(NROS_RMW_EXTRA_LINK_LIBS "nros_rmw_cyclonedds;ddsc;stdc++" PARENT_SCOPE)
packages/cli/cargo-nano-ros/src/rmw_resolver.rs:186:   (the same line, emitted into generated cmake)
```

Nothing consumes it. So the cyclone archive reaches the link some other way —
plain link flags or a target-level `add_dependencies` — and a path that does not
go through `target_link_libraries` is precisely how a lib ends up order-only:
CMake models "link this" as an implicit dep, but "depend on this target" as
order-only.

## Resolution

`LINK_DEPENDS` on the consuming target, set in `nano_ros_link_rmw`
(`cmake/NanoRosLink.cmake`) — the one seam every consumer passes through,
whatever verb created the target:

```cmake
if(TARGET nros_rmw_${_chosen})
    set_property(TARGET ${TARGET} APPEND PROPERTY
        LINK_DEPENDS "$<TARGET_FILE:nros_rmw_${_chosen}>")
endif()
```

That adds the file edge the flag cannot carry, without touching the link LINE.

Verified behaviourally, not just structurally:

```console
$ ninja -C examples/native/c/talker/build-cyclonedds -t query c_talker
    | …/libnros_rmw_cyclonedds.a      <- implicit now (was only ||)
    || …/libnros_rmw_cyclonedds.a

$ cmake --build …/build-cyclonedds -j   # rc=0
$ touch packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/vtable.cpp
$ cmake --build …/build-cyclonedds -j   # rc=0
c_talker 1786089172 -> 1786089191        RELINKED
```

### Two approaches that do NOT work, recorded so they are not retried

* **`INTERFACE_LINK_DEPENDS` on the `NanoRos` INTERFACE library.** Looks like the
  natural home. On CMake 3.22 the edge stayed `||` after a clean reconfigure.
* **Also `target_link_libraries(NanoRos INTERFACE nros_rmw_cyclonedds)`** next to
  the raw flag, to get the dependency "for free". This BREAKS THE LINK:
  `undefined reference to ddsrt_malloc/calloc/free`. The archive must stay
  inside the `--whole-archive … libddsc.a --no-whole-archive` group; adding it
  separately reorders ld's single pass. The root CMakeLists already warns about
  this ordering for the platform lib — the same trap, one library over.

The lesson generalises past cmake: the dependency graph and the command line are
separate channels here, and a fix aimed at the wrong one either does nothing
(`INTERFACE_LINK_DEPENDS`) or breaks the build (`target_link_libraries`).

## Fix direction (original analysis)

Attach the RMW archive with `target_link_libraries` on the example targets so
CMake emits it as an implicit (`|`) input. Verify with `ninja -t query <bin>`
that the `.a` moves from `||` to `|`, and regression-test by touching a backend
source and confirming the binary relinks.

Worth auditing the same way: every other `||` entry in those link lines, and the
zenoh/XRCE equivalents — this was found on cyclone only because that is where
`rmw_ret.h` happened to land.

## Scope

Every C/C++ example fixture that links an RMW archive. The tests do not silently
pass on stale binaries — the staleness probe catches them, which is why this
surfaced at all — but they cannot run until someone wipes, and the failure text
sends them at a command that will not help.

## Original (incorrect) framing, kept deliberately

Filed as: "the probe examines 8503 inputs and flags a header the leaf does not
include; the build graph is precise and says nothing to do; both are right and
they deadlock." Two errors in that. `rmw_ret.h` IS a dependency of objects
feeding the binary (`ninja -t deps` lists it five times under objects that
`ninja -t inputs c_talker` also lists), and `cmake --build` doing nothing is not
correctness — it is the missing edge. The evidence that looked like "probe too
broad" was: an unscoped `-t deps` read, and a `cmake --build` that exits 0.
Neither distinguishes "no work needed" from "no edge to notice the work".

## Audit (2026-08-07) — was any other backend affected?

Swept every built example: **348 build dirs, 391 executables**, looking for an
archive that has an order-only edge and NO implicit one.

* **zenoh / XRCE — not affected, structurally.** Since phase-241 D3 those
  backends are BUNDLED into the umbrella staticlib (`libnros_c.a` /
  `libnros_cpp.a`) via a cargo feature; there is no separate backend archive and
  therefore no raw-flag link. Verified on the zenoh and XRCE talkers: every `.a`
  carries an implicit edge, and the order-only-only entries are phony/utility
  targets (config-header generation, corrosion's cargo custom command), which is
  what order-only is FOR.
* **cyclonedds — the only one**, as diagnosed.

22 executables still show the old shape, all ThreadX × cyclonedds
(`qemu-riscv64-threadx`, `threadx-linux`). Those are stale build dirs, not a gap
in the fix: `cmake/platform/nano-ros-threadx.cmake:330` routes every app target
through `nano_ros_link_rmw`, which is where `LINK_DEPENDS` now lives, and their
`build.ninja` files date from 2026-08-01 — before the fix. The native cyclone
dir reconfigured after it shows the implicit edge. They pick it up on their next
reconfigure.

Sweep command, worth re-running after any link-wiring change:

```python
# for each examples/**/build.ninja, for each C/CXX_EXECUTABLE_LINKER target:
#   ninja -t query <exe>; flag archives in the `||` set but not the `|` set
```
