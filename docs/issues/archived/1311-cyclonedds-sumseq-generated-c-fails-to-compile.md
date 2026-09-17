---
id: 1311
title: "Cyclone RMW test suite: the idlc-generated descriptors had no owning
  target, so a parallel make ran the generator up to eleven times at once into
  the same files"
status: resolved
type: bug
area: rmw, cyclonedds, build
related: [issue-0319, issue-0834, phase-453]
resolved_in: "2026-09-18 — check-cmake-generated-source-owners + one OBJECT-library owner per generated set"
---

## Verdict

**A real defect on `main`, not a worktree-provisioning artifact.** It reproduced
on the first attempt on a clean `origin/main` checkout. What made it look like a
provisioning defect is that the manifestation requires a **cold** build: a
long-lived checkout has the generated files already up to date and re-runs no
generation rule at all.

The filing's hypothesis 1 ("the idlc that ran is not the one built from this
checkout") was **measured and ruled out** — see below.

## Root cause

`nros_rmw_cyclonedds_idlc_compile` / `…_generate_from_msg` return a list of
generated `.c` files, and `tests/CMakeLists.txt` listed each returned list in
the source lists of **many** targets: `_test_string_sources` in 10,
`_addtwoints_sources` in 9, `_from_msg_sources` in 4,
`_example_addtwoints_sources` in 2.

Under the Makefile generators an `add_custom_command(OUTPUT …)` is not a node in
one global graph: CMake copies the rule into the `build.make` of **every** target
that lists an output as a source, and `CMakeFiles/Makefile2` drives each target
through its own sub-make. `cmake --build --parallel` (unbounded `make -j`, which
is what `just cyclonedds build-rmw` uses when `NROS_JOBSERVER` is unset) then
runs the same command **once per consuming target, concurrently**, every copy
writing the same file. CMake's own `add_custom_command` documentation names the
misuse: *"Do not list the output in more than one independent target that may
build in parallel or the instances of the rule may conflict."*

Ninja is immune (one edge per output), which is why every Zephyr/west consumer
of the same helpers is clean.

## Measured evidence

Branch `fix/1311-cyclone-sumseq` at `origin/main` (`fa6cf68d2`), submodule
`third-party/dds/cyclonedds` initialised non-recursively at its recorded pin
`67ff7518` (`0.10.5-31-g67ff7518`, unmoved). `just check rmw-cyclonedds`,
cold, exit 2. Log: `tmp/1311-gate-run1.log`.

Duplicate concurrent generation, counted from the build's own progress lines:

```
idlc test_string.idl                          10 invocations
idlc AddTwoInts.idl                           11 invocations
idlc SumSeq.idl                                9 invocations
msg_to_cyclone_idl nros_test/srv/SumSeq.srv    9 invocations
msg_to_cyclone_idl nros_test/srv/AddTwoInts.srv 11 invocations
```

and the rule text itself present in nine `tests/CMakeFiles/*/build.make` files,
each a full recipe rather than a dependency line.

The failure, verbatim (the sibling of the filed `SumSeq.c` — same rule, same
race):

```
[ 91%] Building C object tests/CMakeFiles/nros_rmw_cyclonedds_service_smoke.dir/cyclonedds-from-msg/nros_test/gen/SumSeq_register_0.c.o
…/build/tests/cyclonedds-from-msg/nros_test/gen/AddTwoInts.c:11:14: error: unknown type name ‘uint32_t’
   11 | static const uint32_t nros_test_srv_dds__AddTwoInts_Request__ops [] =
…/gen/AddTwoInts.c:14:3: error: ‘DDS_OP_ADR’ undeclared here (not in a function)
…/gen/AddTwoInts.c:28:13: error: ‘NULL’ undeclared here (not in a function)
…/gen/AddTwoInts.c:21:7: error: unknown type name ‘dds_topic_descriptor_t’
gmake[2]: *** [tests/CMakeFiles/nros_rmw_cyclonedds_service_concurrent.dir/build.make:121: …/gen/AddTwoInts.c.o] Error 1
```

That is a `.c` whose `#include "AddTwoInts.h"` opened the header **mid-write**,
before the writer had reached its `#include "dds/ddsc/dds_public_impl.h"` — so
every name that header supplies is undeclared while the include itself
succeeds. The complementary interleaving truncates the `.c`, which costs the
descriptor and lands as the other reported shape,
`undefined reference to 'nros_test_srv_dds__SumSeq_{Request,Response}__desc'`.
The link error is that cascade, not a second bug.

The corruption is **transient**, not absorbing (unlike issue 0834): a later
writer completed the file, so the artifacts on disk after the failed build are
intact and a second `make` succeeds. That is exactly why it read as
intermittent and as somebody else's provisioning.

### What was ruled out

- **A mismatched `idlc`.** The fresh worktree selected
  `~/.nros/sdk/cyclonedds/0.10.5-nros1/bin/idlc` (SDK store) where the
  long-lived checkout's sticky cache holds `/opt/ros/humble/bin/idlc`. Both were
  run on the same `SumSeq.idl`: byte-identical output apart from the output path
  in the banner and the path-derived include guard. Not the cause.
- **Include-path breakage / a missing transitive `<stdint.h>`.** The generated
  header includes `"dds/ddsc/dds_public_impl.h"`, and the compile line carries
  all four Cyclone include roots (source `ddsc`/`ddsrt` plus the two build-dir
  ones). A genuinely missing header is a fatal "No such file or directory", not
  an undeclared-name list.
- **A header-guard collision from the worktree path.** idlc mangles every
  non-alphanumeric to `_`, verified on a path containing `.claude` and
  `agent-…`; guards stay distinct per file.
- **Worktree provisioning.** The long-lived checkout's `build.make` files carry
  the identical duplication, same generator (`Unix Makefiles`). Only the
  coldness differs.

## Fix

`nros_rmw_cyclonedds_own_generated_sources(<var> <owner> SOURCES … )` in
`packages/rmw/cyclonedds/nros-rmw-cyclonedds/cmake/NrosRmwCycloneddsTypeSupport.cmake`
— one OBJECT library owns each generated set and the variable becomes
`$<TARGET_OBJECTS:<owner>>`, so consumers list it exactly where they listed the
sources. OBJECT and not STATIC because the `_register_*.c` TUs are reached only
through `__attribute__((constructor))` and an unreferenced archive member is not
pulled in. Applied to all four sets in `tests/CMakeLists.txt`; `graph_query`
links the owner instead of re-listing it, because `$<TARGET_OBJECTS:…>` is
documented for `add_library`/`add_executable` source lists only.

Deliberately **not** applied inside the helpers: their far consumers (the Zephyr
action-type module, the per-example leaves) add the include dirs the generated
TUs need to their own target *after* the call, so an owner library created in
there would compile without them — and each of those callers feeds exactly one
target, so none is exposed. Swept: of 27 generated source sets in the tree,
these four were the only ones with more than one consumer.

Gate: **`check-cmake-generated-source-owners`** (fast lane, buildless,
self-testing) — it follows the two helpers' output variables and fails when one
is a source of more than one target. Verified to flag the pre-fix file with
exactly the counts the build log measured (10 / 9 / 4 / 2).

## Acceptance

`just check rmw-cyclonedds` after the fix: 33/33 ctest, exit 0, with each
generation command running **once** (`idlc test_string.idl` 1×, each
`msg_to_cyclone_idl` 1×) where the failing run ran them 9–11 times. Each rule
now appears in exactly one `build.make`, the owner's.
