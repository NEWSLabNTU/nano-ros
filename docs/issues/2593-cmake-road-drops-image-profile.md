---
id: 2593
title: "`[image.<id>] profile` is silently ignored on the CMake road: `nros build` always configures Release"
status: open
type: bug
area: [cli, cmake]
severity: medium
found: 2026-11-15
related: [phase-478, issue-2582]
---

GitHub: NEWSLabNTU/nano-ros#3899

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md).**
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
