---
id: 688
title: "`nros-c` fails `#[panic_handler]` in the threadx-rv64 RUST Cyclone leaf — the last row standing after #0678's wipe, and the only one that is not staleness"
status: open
type: bug
severity: high
area: build, boards
related: [issue-0678, issue-0668, issue-0666, issue-0664]
---

## Symptom

`just threadx_riscv64 build-fixture-extras` stops at:

```
  → examples/qemu-riscv64-threadx/rust/talker (-DNROS_RMW=cyclonedds, build-cyclonedds/)
   Compiling nros-c v0.5.0 (/home/aeon/repos/nano-ros/packages/api/nros-c)
error: `#[panic_handler]` function required, but not found
error: could not compile `nros-c` (lib) due to 1 previous error; 2 warnings emitted
error: recipe `build-fixture-extras` failed with exit code 101
```

This is the failure [#0678](0678-threadx-rv64-cpp-cyclone-emutls-errno-undefined.md)
predicted in its closing section ("Still failing, and NOT this issue") and asked
to have filed separately. Filed here with the cause narrowed.

## What #0678's fix DID fix, measured

#0678's reopen established that a toolchain-file change cannot reach a build
tree that already exists (`CMAKE_C_FLAGS` is seeded from `CMAKE_C_FLAGS_INIT` on
the FIRST configure only, and CMake rewrites `CMakeCache.txt` every configure so
its mtime looks current). It deleted the two **C** Cyclone dirs. The three
others were left.

Deleting all five threadx-rv64 Cyclone trees and rebuilding fixes four of them:

| leaf | after wipe |
| --- | --- |
| `c/talker/build-cyclonedds` | BUILT (08-19 07:18) |
| `c/listener/build-cyclonedds` | BUILT (08-19 07:18) |
| `cpp/talker/build-cyclonedds` | BUILT (08-19 07:18) |
| `cpp/listener/build-cyclonedds` | BUILT (08-19 07:18) |
| `rust/talker/build-cyclonedds` | **still fails** |

So the C/C++ residue WAS staleness and is now gone; the fix is complete for
those rows and only needed the remaining dirs wiped. The Rust leaf's tree was
deleted in the same sweep, so what remains is a fresh-configure failure — a real
defect, not a museum tree.

## The distinguishing fact

`nros-c` is the **C API** crate, and it is being compiled inside the **Rust**
leaf. The three C/C++ leaves compile the same crate with byte-identical
features and succeed:

```
--no-default-features --features=ros-humble,rmw-cffi,alloc,platform-threadx,panic-platform
```

`panic-platform` is present, so the feature that is supposed to supply the
handler IS enabled and the handler still is not found. That is the part worth
starting from: the same crate, same features, same target, differing only in
which leaf's build tree it is compiled under.

Adjacent context that probably matters:

* [#0666](0666-threadx-zenoh-and-cyclonedds-build-paths-diverge.md) — this board
  builds one example TWO ways, cargo for zenoh and CMake for Cyclone, and the
  RMW picks the build system. The failing row is the Cyclone (CMake) path of a
  leaf whose zenoh path is cargo.
* [#0668](0668-threadx-rv64-example-shape-differs-from-every-other-standalone.md)
  — threadx-rv64 is the only standalone example owning two entry points, so it
  is the only one where the panic handler has a PLACEMENT question. A Rust leaf
  that also pulls in `nros-c` is exactly that shape.

## Not verified

* Whether the Rust leaf needs `nros-c` at all, or whether it is pulled in by the
  Cyclone CMake path as a side effect. If the latter, the fix may be to not
  build it there rather than to give it a handler.
* Whether `panic-platform` resolves to a provider in this leaf's graph. The
  feature being ON is established; that the crate providing `#[panic_handler]`
  is actually linked into this compilation is not.
* Whether the zenoh path of the same leaf has the same shape and merely never
  reaches the failing compile.

## Method note, worth repeating

Two attributions of this failure were WRONG before the artifact check settled
it. `ninja` interleaves parallel output, so "the last cargo invocation printed
before the error" named `cpp/listener` on one run and `cpp/talker` on the next —
both innocent, and both had built successfully. The failure appeared to MOVE
between rows, which reads as a race and is not one. What resolved it was
checking which binaries exist on disk and reading the build's own
`→ examples/...` banner line, not the surrounding log. Anyone continuing this
should attribute by artifact, never by log adjacency.
