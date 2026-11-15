---
id: 2593
title: "`[image.<id>] profile` is silently ignored on the CMake road: `nros build` always configures Release"
status: resolved
type: bug
area: [cli, cmake]
severity: medium
found: 2026-11-15
resolved: 2026-11-15
resolved_in: "the change that resolved it (phase-478 I9)"
related: [phase-478, issue-2582]
---

GitHub: NEWSLabNTU/nano-ros#3899

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md).**
Seen on main 37232ecff1 with the Autoware Safety Island port.

## What happens

`[image.<id>] profile` is accepted on an image whose driver is CMake, but it
does not reach the configure. `nros build` passes `-DCMAKE_BUILD_TYPE=Release`
for every non-posix image (`cmd/build.rs`, the `platform != "posix"` default),
and `image.profile` is read only on the cargo road (`--profile`). The row
changes nothing, not even the build coordinate:

```toml
[image.an536]
board = "mps3-an536-freertos"
profile = "nros-relwithdebinfo"   # also tried "RelWithDebInfo"
```

`build/<coord>/cmake/CMakeCache.txt` holds `CMAKE_BUILD_TYPE:STRING=Release`
and `NROS_CARGO_PROFILE:STRING=release`, and the ELF is byte-identical to the
build without the row. The book's system.toml reference says `profile` is the
"cargo or CMake build profile name".

## The class

The same as issue 2582:
an image fact that one road accepts and another drops without a word. A stated
fact either reaches every road, or the road that cannot apply it refuses it.

## Direction

Read the profile through the image facts the CMake road already asks for. On
that road it is two values: `CMAKE_BUILD_TYPE`, and the cargo profile of the
runtime crates the configure builds through Corrosion (`NROS_CARGO_PROFILE`).
Map the value to both. Make it part of the build coordinate, so two profiles
of one image do not share a build directory. Refuse a value that maps to
neither, naming the CMake spelling (`-- -DCMAKE_BUILD_TYPE=<type>`). The book
row then states which values each road takes.

Workaround: `nros build an536 -- -DCMAKE_BUILD_TYPE=RelWithDebInfo`.

## Resolution

phase-478 I9. One mapping, `nros_cargo_profile::cmake_build_type`, reads a
`profile` in either vocabulary the book offers: a cargo profile from the
profile table (`dev`, `release`, `nros-relwithdebinfo`, `nros-minsizerel`) or
a CMake build type (`Debug`, `Release`, `RelWithDebInfo`, `MinSizeRel`,
case-insensitive).

- Both CMake configures `nros build` writes pass
  `-DCMAKE_BUILD_TYPE=<type>` and `-DNROS_CARGO_PROFILE=<profile>` from it
  (`image_build_type_args`), so C, C++ and the Rust runtime build as one
  profile. These are the workspace image and the single-package leaf, which
  now reads `profile` too (`LeafSystem::profile`). With no `profile`, the
  board default (Release on a board, unset on a host) stands, and a
  `-DCMAKE_BUILD_TYPE` after `--` still wins.
- The profile is part of the configuration key, so two profiles of one image
  do not share a configure. An image that states none keeps its coordinate.
- A profile no build type maps to (a user's own cargo profile) is refused on
  the CMake road, naming the `-- -DCMAKE_BUILD_TYPE=<type>` spelling.
- The west road passes no build type: Zephyr takes its optimisation from
  Kconfig. It refuses a stated `profile` and names the Kconfig symbols to
  state in an `os_config` fragment.
- The book's system.toml row says what each road does with the value.

Tests: `an_image_profile_names_one_cmake_build_type` (nros-cargo-profile),
`an_image_profile_reaches_the_cmake_configure`,
`a_profile_a_cmake_build_cannot_apply_is_refused` and
`the_profile_is_part_of_the_configuration_key` (cmd/build.rs).
