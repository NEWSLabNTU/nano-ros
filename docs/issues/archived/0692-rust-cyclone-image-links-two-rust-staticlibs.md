---
id: 692
title: "The threadx-rv64 Rust+CycloneDDS image links TWO Rust staticlibs, so `#[panic_handler]` has no correct owner — one of them must stop being a final artifact"
status: resolved
type: bug
severity: high
area: build, boards
related: [issue-0666, issue-0668, issue-0618, issue-0678, rfc-0077, phase-366]
---

## Symptom

`just threadx_riscv64 build-fixtures` fails on the Rust leaves' CycloneDDS
builds:

```
→ examples/qemu-riscv64-threadx/rust/talker (-DNROS_RMW=cyclonedds, build-cyclonedds/)
   Compiling nros-c v0.5.0
error: `#[panic_handler]` function required, but not found
```

The third distinct failure this platform has surfaced in sequence (0674 → 0678 →
this), each standing in front of the next. The libc halves are fixed and
verified; this one is not a libc problem.

## What makes it unfixable by wiring

That image links **two independent Rust final artifacts**:

* the leaf crate's staticlib (`qemu-riscv64-threadx-talker`), imported by
  `nros_threadx_rv64_rust_cyclone_app`, and
* `nros-c`'s staticlib, reached as `NanoRos::NanoRos`.

`nros-c` is `crate-type = ["staticlib", "cdylib", "lib"]`, so rustc demands the
lang item while compiling it — and the leaf is a staticlib for the same reason.
**Each needs its own `#[panic_handler]` at COMPILE time, and the image must end
up with exactly ONE at LINK time.** Those two requirements cannot both hold
while both crates are final artifacts.

Demonstrated by moving the handler and watching the failure move with it:

| configuration | result |
| --- | --- |
| leaf declares (`panic_to_platform!()`), nros-c does not | `nros-c` fails to compile |
| nros-c declares (`panic-platform` via the helper), leaf gated off | the LEAF fails to compile |

Both were built and measured, from wiped `build-cyclonedds` trees. Giving both
the handler is not a third option: two `#[panic_handler]`s in one link is the
duplicate the whole of phase-366 exists to prevent.

## Why the C/C++ leaves are fine

They link ONE Rust staticlib. `nano_ros_entry(... PANIC platform)` applies
`panic-platform` to `nros_c`/`nros_cpp` — and `nros-cpp`'s `panic-platform`
FORWARDS to `nros-c`'s rather than defining a second handler, so the image gets
exactly one. Verified on the same run:

```
--features=ros-humble,rmw-cffi,alloc,platform-threadx,panic-platform --package nros-c
--features=ros-humble,rmw-cffi,alloc,platform-threadx,panic-platform --package nros-cpp
```

A Rust entry never calls `nano_ros_entry`, which is why nothing applied the
feature on that path — but adding it there only moves the failure, as the table
above shows.

## This is issue 0666's shape, and that is the direction

[Issue 0666](0666-threadx-zenoh-and-cyclonedds-build-paths-diverge.md) records that this one
example is built two ways — cargo for zenoh, CMake for CycloneDDS. The zenoh path
links no `nros-c`, so the leaf is the only final artifact and declares its own
ending correctly. Only the CMake path has two.

So the fix is not a feature flag. One of the two must stop being a final
artifact in this image:

1. **Link `nros-c` as an rlib into the leaf's staticlib** rather than as a
   separate archive, so there is one Rust artifact and one handler. Closest to
   how every other Rust image on this tree is shaped.
2. **Make the leaf a `cdylib`/object rather than a staticlib** on this path, so
   `nros-c` is the only final artifact and supplies the ending — the shape the
   C/C++ leaves already have, with the Rust crate as a library rather than an
   image.
3. **Drop the CMake path** for Rust leaves (0666's own question), leaving cargo
   as the single way a Rust image is built here.

Each is a real change to how this platform composes an image, which is why it is
filed rather than patched.

## Do not retry

* Gating the leaf's `panic_to_platform!()` behind a default feature and letting
  the corrosion import's `NO_DEFAULT_FEATURES` drop it. Tried: the zenoh fixture
  rows ALSO pass `no_default_features = true`, so the seam does not separate the
  two paths, and naming the feature explicitly in those rows then just moves the
  error to the leaf's own staticlib (row 2 of the table).
* Applying `panic-platform` to only the `nros_c` pair. `nros-cpp` is imported
  too, and listing only two of the four spellings leaves its staticlib without
  the lang item.


---

## RESOLVED 2026-08-19 — the wiring fix works, because the two handlers do not collide

This issue concluded "unfixable by wiring", from a table showing that whichever
crate is DENIED the handler fails to compile, and that giving BOTH the handler
"is not a third option: two `#[panic_handler]`s in one link is the duplicate the
whole of phase-366 exists to prevent."

The compile-time half of that is exactly right and was reproduced. The link-time
half does not hold on this target, and the third option is what shipped.

Give BOTH crates the handler, and drop `NanoRos::NanoRos` from
`nros_threadx_rv64_rust_cyclone_app`'s link line. The image builds and links:

```
$ riscv-none-elf-nm libnros_cpp.a | grep rust_begin_unwind
0000000000000000 t _RNvCs...rust_begin_unwind      <- LOCAL

$ riscv-none-elf-nm libqemu_riscv64_threadx_talker.a | grep rust_begin_unwind
0000000000000000 T _RNvCs...rust_begin_unwind      <- GLOBAL
                 U _RNvCs...rust_begin_unwind
```

`t` versus `T` is the whole answer. `nros-cpp`'s provider is internalised to its
own archive, so it serves that crate's panics and is invisible to the link; the
leaf's is the one global provider. The final image contains **exactly one**
global `rust_begin_unwind`, which is the property phase-366 wants — and it is a
property of symbol LOCALITY, not of there being only one final artifact.

So a crate being a final artifact does not by itself put a competing lang item
on the link line. "Each needs its own at COMPILE time, and the image must end up
with exactly ONE at LINK time" is satisfiable, and both requirements hold here
simultaneously.

Two notes on the "Do not retry" list:

* "Applying `panic-platform` to only the `nros_c` pair" — correct, and avoided:
  the seam applies it to all four spellings (`nros_c`, `nros_cpp`,
  `nros_c-static`, `nros_cpp-static`), the same set `nano_ros_entry()` walks.
* The deeper point stands and is recorded on
  [#0666](0666-threadx-zenoh-and-cyclonedds-build-paths-diverge.md): the reason
  this leaf needed a bespoke fix at all is that
  `nros_threadx_rv64_rust_cyclone_app()` is not `nano_ros_entry()`, so it never
  applied a panic policy in the first place. Any machinery added to the entry in
  future will miss this seam again until the two paths converge.

`nros-c` itself is no longer linked into the image (verified: no `libnros_c.a`
on the executable's link line) but is still BUILT, because the root
`add_subdirectory()` puts it in `all`. Making an unlinked artifact stop being
built is a further cleanup, not a correctness requirement.

Fixed by the same commit that resolved the duplicate #0688.
