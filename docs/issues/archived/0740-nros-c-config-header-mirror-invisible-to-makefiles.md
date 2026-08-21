---
id: 740
title: "nros-c's mirrored config header is invisible to the Unix Makefiles generator across directories — consumer entry TUs die with 'No rule to make target'"
status: resolved
type: bug
area: build
related: [issue-0088, issue-0268, issue-0090]
---

# 0740 — config-header mirror: cross-directory OUTPUT vs Makefiles

`packages/api/nros-c/CMakeLists.txt` mirrors the per-build
`nros_config_generated.h` via `add_custom_command(OUTPUT ...)` +
`nros_c_config_header` (the 0088/0268 producer→consumer edge), and
NanoRosNodeRegister/entry wiring puts a FILE-level `OBJECT_DEPENDS` on the
mirrored path (0090) so a TU cannot compile against a stale mirror.

That file-level edge only works under generators that resolve
cross-directory file dependencies (Ninja). Under **Unix Makefiles**, a
custom-command OUTPUT is only known to the makefile of the directory that
declared it; a consumer TU in another directory that names the file as a
prerequisite gets, on a CLEAN build:

```
gmake[3]: *** No rule to make target
'nano_ros/packages/api/nros-c/include/nros/nros_config_generated.h',
needed by 'src/<entry>/CMakeFiles/<entry>.dir/<entry>_nros_main_generated.cpp.o'.
```

Once the file exists on disk (second `cmake --build`, or pre-building
`nros_c_config_header`), make is satisfied — which is why in-tree lanes
never see it: the fixture flows build stages in an order that leaves the
mirror present, and CLEAN single-shot consumer builds are exactly the
downstream case.

## Repro

Downstream workspace consumer (autoware-safety-island phase-4 W5.a,
`freertos-posix` board, default generator = Unix Makefiles):

```sh
rm -rf build && cmake -S . -B build -DNANO_ROS_PLATFORM=freertos \
  -DNANO_ROS_BOARD=freertos-posix -DNROS_RMW=cyclonedds ...
cmake --build build --target <entry>       # fails as above, clean tree
cmake --build build --target <entry>       # succeeds (mirror now exists)
```

Observed at `b13241d41`.

## Fix shape

The standard CMake pattern for cross-directory generated files: consumers
must depend on the custom TARGET, not (only) the OUTPUT file — e.g. the
entry-emitting verb adds `add_dependencies(<entry> nros_c_config_header)`
AND keeps the OBJECT_DEPENDS for rebuild correctness; or the mirror moves
into the same directory scope the entry is generated from; or the docs
require Ninja for consumer builds (weakest — the verbs work today under
Makefiles except for this one edge). Sweep the class: any other
`OBJECT_DEPENDS` naming a custom-command OUTPUT from another directory has
the same Makefiles hole (0090 lists the sites).

## Downstream workaround (until fixed)

Second build pass, or `cmake --build build --target nros_c_config_header`
before the entry target; ASI's build lane currently rides the
second-pass behavior.

## Resolution (2026-08-21)

Fixed by `5024f2b93`: `_nros_config_header_stamp` (NanoRosNodeRegister.cmake)
gives every consumer directory a LOCAL `add_custom_command(OUTPUT <stamp>)`
whose recipe copies the mirror — so Unix Makefiles has an in-directory rule
for the prerequisite, while the stamp's own deps (the cargo/mirror targets
plus the staticlib FILE) keep it going stale exactly when the mirror does in
both generators (the 0268 museum-mirror hazard stated in the helper).
Applied at all six OBJECT_DEPENDS sites (NodeRegister ×2, Entry ×2,
GenerateInterfaces, plus the C-node arm).

Verified 2026-08-21 on the repro shape: clean single-shot
`cmake -G "Unix Makefiles"` + `cmake --build` of a scaffolded
`multi-node-workspace-cpp` consumer — `robot_entry_nros_main_generated.cpp.o`
(the failing TU class) compiles on the FIRST pass; the log shows
`config-header stamp nros_config_generated (issue 0740)` ordering the mirror.
ASI's second-pass workaround can retire.
