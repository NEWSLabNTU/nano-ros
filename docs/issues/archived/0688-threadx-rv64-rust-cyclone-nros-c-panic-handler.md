---
id: 688
title: "RETIRED, duplicate of #0692 — `nros-c` fails `#[panic_handler]` in the threadx-rv64 RUST Cyclone leaf — the last row standing after #0678's wipe, and the only one that is not staleness"
status: resolved
type: bug
severity: high
area: build, boards
related: [issue-0678, issue-0668, issue-0666, issue-0664]
---

## Retired as a duplicate of #0692

Filed 2026-08-19 for the threadx-rv64 Rust Cyclone `#[panic_handler]` failure.
[#0692](../0692-rust-cyclone-image-links-two-rust-staticlibs.md) was filed
independently for the same failure and landed on `main` first, so it is
canonical and carries the resolution. This file is kept for the diagnosis
history below (in particular the measured attribution method) and for the
`#0678` cross-reference that asked for it.

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
leaf. The C/C++ leaves compile the same crate and succeed.

**CORRECTED after diagnosis** — this section originally claimed the two leaves
pass byte-identical features, quoting:

```
--no-default-features --features=ros-humble,rmw-cffi,alloc,platform-threadx,panic-platform
```

That line was read out of an interleaved build log and belongs to a C++ leaf.
The Rust leaf's ACTUAL invocation, from its own `build.ninja`, is:

```
--no-default-features --features=ros-humble,rmw-cffi,alloc,platform-threadx
```

— no `panic-platform`. The features were never identical, and the difference IS
the bug. See the resolution below.

## Method note, worth repeating

Two attributions of this failure were WRONG before the artifact check settled
it. `ninja` interleaves parallel output, so "the last cargo invocation printed
before the error" named `cpp/listener` on one run and `cpp/talker` on the next —
both innocent, and both had built successfully. The failure appeared to MOVE
between rows, which reads as a race and is not one. What resolved it was
checking which binaries exist on disk and reading the build's own
`→ examples/...` banner line, not the surrounding log. Anyone continuing this
should attribute by artifact, never by log adjacency.


---

## RESOLVED 2026-08-19 — the seam never applied a panic policy, because it is not an entry

`nros-c` is imported with `--no-default-features`, which strips its own
`default = ["panic-platform"]`, and the `#[panic_handler]` in
`packages/api/nros-c/src/lib.rs:165` is gated on exactly that feature. A
staticlib is a FINAL artifact, so rustc requires the handler — hence
"required, but not found", four crates away from anything that names the cause.

Every other leaf gets the feature because every other leaf goes through
`nano_ros_entry()`, which defaults `PANIC` to `platform`, maps it with
`nros_panic_policy_feature()`, and appends it to the imported nros-c/nros-cpp
with `corrosion_set_features()` — erroring if it cannot. The threadx-rv64 Rust
Cyclone leaf goes through `nros_threadx_rv64_rust_cyclone_app()` instead, a
bespoke board seam that never had that step.

**This is [#0666](0666-threadx-zenoh-and-cyclonedds-build-paths-diverge.md) with
a price tag.** The one leaf in the tree with its own build path is the one leaf
that silently missed machinery every other leaf gets. The divergence did not
merely cost tidiness; it cost a build.

Fixed in the seam by mirroring the entry's logic, including its fail-loudly
branch. Also dropped `NanoRos::NanoRos` from that seam's link line: a Rust app
never calls the C API (this leaf's cargo graph does not mention `nros-c`, and
its zenoh half builds without it), and linking it put a SECOND Rust staticlib on
a C link line, which `CMakeLists.txt` states must never happen. The leaf now
links clean without it — 0 undefined symbols — which is what verifies the drop.

### Two hypotheses that were wrong, recorded so they are not re-run

* **The link line.** Dropping `NanoRos::NanoRos` alone did NOT fix it. The root
  `add_subdirectory()` puts `nros_c-static` in `all`, so removing the link edge
  never stopped the crate being BUILT. A link edge and a build edge are
  different things, and only the second one matters to a compile error.
* **Stale Corrosion.** The SDK store held 0.5.0 (its `.installed-version` stamp
  claiming `v0.5.1`) against a `v0.6.1` pin, and Corrosion < 0.6.0 shares one
  cargo target-dir across workspace roots — which this leaf uniquely has two of
  (its own bare `[workspace]` plus the nano-ros root). That is issue 0500's
  hazard and it was really present; provisioning `v0.6.1` changed nothing here.
  Worth keeping, not the cause.

  **CORRECTION (2026-08-19).** This originally added "the stamp disagreed with
  the content it stamped, which is its own small defect in the installer." That
  is wrong and there is no such defect. The stamp records the upstream TAG; the
  `CorrosionConfigVersion.cmake` records the CMake PROJECT VERSION, and
  upstream's `v0.5.1` tag declares `VERSION 0.5.0` (verified by cloning the tag)
  — upstream simply did not bump it. Two different version vocabularies, both
  accurate, which `nros setup`'s `tool_pin_status` already reads deliberately.
  The real gap was that nothing ASKED before a build; a probe now does
  (`scripts/check-tier-preconditions.sh`).

### Method

Three attributions of this failure were wrong before the build's own
`→ examples/...` banner and the leaf's `build.ninja` settled it. `ninja`
interleaves parallel output, so the last cargo invocation printed before an
error belongs to whatever finished last, not to what failed. The feature list
that finally explained everything came from `grep 'package nros-c'
<leaf>/build.ninja` — the command as CONFIGURED, not as logged.
