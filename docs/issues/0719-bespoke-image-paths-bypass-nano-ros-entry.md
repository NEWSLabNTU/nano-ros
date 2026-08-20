---
id: 719
title: "Seven image-producing cmake paths link `NanoRos::NanoRos` without going through `nano_ros_entry()`, so each one re-earns whatever the entry applies — twice already"
status: open
type: tech-debt
area: build, integrations
related: [issue-0666, issue-0692, issue-0688, issue-0700, phase-369, rfc-0077]
---

## What this is

[#0666](archived/0666-threadx-zenoh-and-cyclonedds-build-paths-diverge.md) was
closed by phase-369, which unified the ThreadX-RV64 leaf onto one CMake path.
Its closing sentence is the general statement:

> a bespoke path silently misses machinery the shared one applies

That is true of more than the leaf it was scoped to. `nano_ros_entry()` is where
an image's cross-cutting facts get applied — today the panic policy, and
whatever is added next — and `nano_ros_add_executable()` delegates to it
(`NanoRosVerbs.cmake:157`), so ~160 call sites are covered by construction.

These are not:

| path | applies the panic policy? |
| --- | --- |
| `cmake/board/nano-ros-board-riscv64-qemu.cmake` | yes — added by #0688 |
| `integrations/nano-ros/CMakeLists.txt` (ESP-IDF component shim) | yes — added by #0700 |
| `cmake/platform/nano-ros-nuttx.cmake` | no |
| `examples/templates/cpp-port-minimal-publisher/CMakeLists.txt` | no |
| `examples/templates/rclcpp-compat-smoke/CMakeLists.txt` | no |
| `packages/testing/nros-tests/fixtures/cmake_add_subdirectory_smoke/CMakeLists.txt` | no |
| `cmake/compat/NrosRclcppCompat.cmake`, `cmake/compat/stubs/Findrclcpp.cmake` | no |

(`nano_rosConfig.cmake` is in the mechanical sweep but is the package config, not
an image path.)

The first two carry the policy only because a build broke and someone put it
there. Each was found the same way: a `#[panic_handler]` error four crates from
its cause.

## Why the ones marked "no" are not (yet) broken

Because they are C or C++ leaves whose `nros-c` import happens to arrive with
`panic-platform` still on, or they are compile-only smoke fixtures that never
link a final image. That is a property of today's feature resolution, not a
guarantee any of them states. The two that DID break broke when something
upstream changed how `nros-c` is imported — `--no-default-features` strips the
crate's own `default = ["panic-platform"]`, and nothing at these sites says what
should happen then.

So this is not "seven latent bugs". It is: **seven places where the next
cross-cutting fact added to `nano_ros_entry()` will not arrive**, and the failure
will again surface far from the cause.

## Evidence it recurs

* **#0692 / #0688** — the threadx-rv64 Rust Cyclone seam. Cost: a red platform,
  and two wrong diagnoses before the cause was found.
* **#0700** — the ESP-IDF component shim, a DIFFERENT path, same missing policy,
  found a day later while debugging something else. It had also been failing
  silently: the fixture builder swallowed it (`built (0/1)` + `|| true`).
* Phase-369 records a third of the same shape that had nothing to do with panic
  policy: `build-zenoh/` containing an ELF named `..._cyclonedds`, because the
  leaf hardcoded its target name. No gate would have caught that either.

## Directions

Not a plan; the choice is a maintainer's.

* **Make the entry reachable from these paths.** `nano_ros_entry()` is
  entry-package shaped (NAME/BOARD/LAUNCH/MODEL/BRINGUP), which is why a board
  seam and an ESP-IDF component cannot simply call it. Extracting the
  cross-cutting half — "given an imported nros-c/nros-cpp, apply the image's
  policies" — into a function both the entry and these paths call is the
  smallest change that makes the class impossible rather than repeatedly fixed.
* **Gate it.** A check that every image-producing path applying `NanoRos::` also
  applies the policy. Cheap, and catches the next site at review instead of at a
  broken build — but it enforces a rule rather than removing the need for one.
* **Converge the paths** (phase-369's answer for its leaf). Correct where it is
  affordable; the ESP-IDF shim and the NuttX platform file exist because those
  build systems own the image, so they cannot all collapse into the entry.

**A note on sweeping this.** The mechanical grep for "calls the shared verb"
initially EXCLUDED the ESP-IDF shim — because a comment I had written in it
mentioned `nano_ros_entry()`. A textual gate keying on the name rather than on a
call is exactly the sort that reports a clean sweep over a site it never
examined (issue 0196's rule). Whatever gate lands here should key on something
structural.
