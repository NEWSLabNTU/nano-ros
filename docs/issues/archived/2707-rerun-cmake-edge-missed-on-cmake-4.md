---
id: 2707
title: "Under CMake 4.x no reader finds the `RERUN_CMAKE` edge, so a Zephyr image's configure inputs are unwatched by the fixture probe and the fixture signature"
status: resolved
resolved_in: "the change that files this issue"
type: bug
severity: high
area: [testing, build, cmake]
related: [1849, 0196]
found: 2026-11-18
---

**Home:** no phase. Found by a tier-2 run (`just ci matrix`) on newslab-118,
CMake 4.4.4.

## Symptom

`nros-tests::zephyr_leaf_staleness::a_shared_cmake_input_marks_the_image_stale`
failed 3 of 3 runs, solo, on a freshly built tree:

```text
editing a shared cmake module did NOT mark the image stale — the
configure-input half is not reaching ninja's RERUN_CMAKE record
```

## Cause

Two readers found the edge that regenerates `build.ninja` by the prefix
`build build.ninja:`:

- `ninja_configure_deps` in `packages/testing/nros-tests/src/fixtures/binaries/mod.rs`,
  the Zephyr fixture freshness probe's configure-input half (phase-363 W5);
- `scripts/build/dep-closure.py`, the dependency closure that a fixture's
  input signature (`source-manifest.sh`) is hashed from.

CMake 3.x writes that edge as `build build.ninja: RERUN_CMAKE | <inputs>`.
CMake 4.x lists every `cmake_install.cmake` as an output beside it:

```text
build build.ninja <build>/cmake_install.cmake <build>/zephyr/cmake_install.cmake …: RERUN_CMAKE | <inputs>
```

So on CMake 4.x neither reader found any edge. The probe watched none of the
shared `cmake/NanoRos*.cmake` modules or the entry template (63 of the
3,508 inputs on `build-ws-c-talker-cyclonedds`; its `.nros-srcbaseline` held
0 of them). An edit to one left every Zephyr image reading fresh, which is the
silent-fresh shape of issue 0196. The fixture signature lost the same inputs,
so a fixture rebuild was not triggered by them either.

The test caught it; nothing else would have, because a missing edge reads as
"no configure inputs", not as an error.

## Where CMake 4 runs (2026-11-18, read from the repo)

- **Every image the repo defines is Ubuntu 22.04 with the distro's `cmake`**,
  and nothing in the repo adds another CMake: no Kitware apt source, no
  `pip install cmake`, no CMake setup action, and `nros-sdk-index.toml` has no
  CMake tool row (only `apt` `cmake` under `[build_type.nros_cmake]` and
  `ci/docker/apt-packages.txt`):
  - `ghcr.io/newslabntu/nano-ros-ci:humble` (`ci/docker/ci-base`) and
    `ci/docker/zephyr-ros` are `FROM ros:humble-ros-base` (jammy);
  - the self-hosted runner container (`runner-container.sh`, golden
    `ci/docker/runner-context.golden/Dockerfile.golden`) is `FROM ubuntu:22.04`
    with apt `cmake`.
  Jammy's own `cmake` is 3.22, so no repo-defined runner carries CMake 4. The
  merge-gating lanes that build Zephyr run in those images, so they have not
  been passing this staleness check vacuously.
- **Not decided by the repo:** a GitHub-hosted job that runs on
  `ubuntu-22.04` / `ubuntu-latest` with no `container:` uses whatever CMake
  the hosted image preinstalls, outside the repo's control. Which of those
  jobs reaches this probe or a fixture signature was not checked job by job.
- **Developer hosts can carry CMake 4.** newslab-118 has
  `4.4.4-0kitware1ubuntu22.04.1` from a host-level Kitware apt source
  (`/etc/apt/sources.list.d/kitware.list`), which is where this was found.
  Every `nros-tests` run and fixture signature on such a host had the gap.
- Not measured here: the CMake 3.22 spelling of the edge. The old prefix match
  was written against it and the new match accepts it (unit test), but no
  CMake 3.x was run on this host.

## Fix

Both readers match the edge by its OUTPUTS: a `build` line whose output list
(before the `:`) contains `build.ninja`. One rule, two spellings, each naming
its twin (`is_build_ninja_edge` in Rust and in Python).

Tests: `the_rerun_cmake_edge_is_found_in_both_cmake_spellings` (both CMake
shapes, plus two lines that only name `build.ninja` as an input) and
`configure_deps_are_read_off_a_cmake_4_edge` (a CMake 4.x `build.ninja`, with
a `$`-continued input). With the fix, `a_shared_cmake_input_marks_the_image_stale`
passed 2 of 2 against the same fixtures that failed it 3 of 3.

Sweep: `git grep -n 'build build.ninja' -- packages scripts just cmake`
(the two readers above; no other reader of the edge).
