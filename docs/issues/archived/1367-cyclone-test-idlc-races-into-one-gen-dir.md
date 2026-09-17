---
id: 1367
title: "two concurrent `idlc` runs write one Cyclone test `gen/` dir: the
  truncated header survives the run that made it, and no later run repairs it"
status: resolved
type: bug
area: build, rmw-cyclonedds
related: [issue-0834, issue-1311, phase-455]
---

## RESOLVED as a DUPLICATE of [issue 1311](1311-cyclonedds-sumseq-generated-c-fails-to-compile.md)

Filed 2026-09-18 from phase-455 W5's run, and **it should not have been filed**:
issue 1311 had the same defect from 2026-09-11, and the fix landed at
`f62c17359a` the same night this was written. Checking `docs/issues/` for an
existing report of a symptom is the step that was skipped — CLAUDE.md says to,
and the cost here was one duplicate rather than a wrong fix.

1311's fix names the mechanism this issue could only guess at, and it is worse
than "two callers agree on an output path": `tests/CMakeLists.txt` listed each
generated-source list in the sources of MANY targets (10, 9, 4 and 2). Under the
Makefile generator an `add_custom_command(OUTPUT …)` is copied into the
`build.make` of every consuming target and `CMakeFiles/Makefile2` drives each
through its own sub-make, so `cmake --build --parallel` ran one generator once
per consuming target, concurrently, all writing the same file — **`idlc
AddTwoInts.idl` eleven times at once**, measured from the failing build's own
progress lines. This issue guessed at two colliding call sites and marked that
NOT CONFIRMED; the real count was eleven, and the cause was target membership
rather than a duplicated call.

What this issue got right and is worth keeping: the state it leaves behind does
not converge. The truncated header is newer than its IDL input, so the codegen
edge reads up to date and no later run repairs it — issue 0834's shape. 1311 is
still `status: open` at the time of writing even though its fix has landed; its
acceptance belongs to whoever filed it, not here.

## Symptom, measured

Observed 2026-09-13 in the box tree while running `just check rmw-cyclonedds`
for phase-455 W5.

`idlc AddTwoInts.idl` ran **twice, concurrently, into one `gen/` directory**.
One writer left a truncated header behind. The next run then failed
*differently* — `undefined reference to …__desc` at link — and **kept failing**:
re-running the gate did not repair it, because the truncated header is newer
than its IDL input, so the codegen edge is up to date and never re-fires.

Clearing that one `gen/` directory by hand made the lane pass: `33/33`.

## Why this is issue 0834's shape and not a plain race

The first failure is a race. The part that costs time is what comes after: the
build reaches a state that **cannot converge on its own**. 0834 is the
canonical case in this tree —

> a mirror dir holds `nros_cpp_config_generated.h.stamp` and NOT the header, so
> cargo is up to date, the build script does not re-emit the byproduct, and
> ninja records its custom command as successful

— and the same sentence holds here with a truncated header in place of a
missing one. CLAUDE.md's `rm -rf` rule has exactly two exemptions for that
reason, and this looks like a third candidate. **It should be fixed as a
dependency/uniqueness defect, not added to the exemption list.**

## Where to look

`nros_rmw_cyclonedds_generate_from_msg`
(`packages/rmw/cyclonedds/nros-rmw-cyclonedds/cmake/NrosRmwCycloneddsTypeSupport.cmake:693`)
picks the generated directory two ways:

```cmake
    if(_arg_GEN_ROOT)
        set(_gen_dir "${_arg_GEN_ROOT}/${_arg_PKG_NAME}/msg")
    else()
        set(_gen_dir "${_arg_OUTPUT_DIR}/gen")
```

So two callers that agree on `GEN_ROOT` + `PKG_NAME`, or on `OUTPUT_DIR`, name
the same output path. `NrosZephyrCycloneddsActionTypes.cmake:30-66` makes four
calls against one `_gen_root`, one per package — correct as long as `PKG_NAME`
differs, which is exactly the invariant nothing checks.

**Not yet confirmed**: which two call sites collided on `AddTwoInts`. The
measurement above is the run's; the mechanism is read off the code and needs the
colliding pair named before a fix. Two custom commands declaring the same
`OUTPUT` is a cmake-level defect ninja will happily schedule twice.

## Fix shape

1. Name the colliding pair first — `ninja -C <build> -t query <header>` and
   `-t commands` on the generated header, which says how many edges claim it.
2. Then make the output path unique per (package, caller) or make the two
   callers share ONE custom command. A shared output with two producers is the
   defect; serialising it with a dependency would only hide the second producer.
3. A truncated-output guard is worth having either way: codegen should write to
   a temp path and rename, so an interrupted writer leaves nothing rather than
   half a header. That is what makes the state converge on the next run even if
   a race remains.

**Acceptance.** The generated header has exactly one producing edge; a killed
codegen run leaves no partial header; and `just check rmw-cyclonedds` passes
twice in a row from a dirty build dir with no manual clearing.
