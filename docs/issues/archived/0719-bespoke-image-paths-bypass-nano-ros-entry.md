---
id: 719
title: "Seven image-producing cmake paths link `NanoRos::NanoRos` without going through `nano_ros_entry()`, so each one re-earns whatever the entry applies — twice already"
status: resolved
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

## Fixed 2026-08-20 — every image path applies an ending, and a gate keeps it that way

Directions 1 and 2, together. Direction 1 alone leaves the next bespoke path free
to skip the applier; direction 2 alone enforces a rule instead of removing the
need for one. Neither is sufficient, which is why both.

**Direction 1 had already landed** (`b60cf8341`): `nros_apply_panic_policy` is
the one implementation, and the entry plus the two paths that had hand-copied
it now call it. What was left is the five paths still marked "no", which this
finishes:

* `cmake/platform/nano-ros-nuttx.cmake` — applied inside
  `nros_platform_link_app()`, the PER-IMAGE seam. NuttX owns the image (its apps
  build system calls that seam per target), so it is the analogue of what #0688
  did for the riscv64 board. The include is at FILE scope, because
  `CMAKE_CURRENT_LIST_DIR` inside a function body resolves at CALL time to the
  caller's directory — the `_NROS_ENTRY_DIR` gotcha CLAUDE.md records.
* `examples/templates/cpp-port-minimal-publisher`,
  `examples/templates/rclcpp-compat-smoke`,
  `packages/testing/nros-tests/fixtures/cmake_add_subdirectory_smoke` — one
  call each, with a guarded include because these are reached several ways
  (`find_package`, `add_subdirectory`, an ament overlay) and not all of them
  have pulled `NanoRosEntry.cmake` first.

**Deliberately NOT applied**, and this is a correction to this issue's own
table: `cmake/compat/NrosRclcppCompat.cmake` and
`cmake/compat/stubs/Findrclcpp.cmake` are an ALIAS LAYER that
`nano_rosConfig.cmake` includes for every consumer, image or not. Applying there
would impose an ending on builds that never link an image, and would FATAL
against an entry that legitimately chose a different one — the applier treats a
second, different policy as a contradiction, correctly, because the staticlib is
shared. They are exempt in the gate WITH that reason.

### The gate, and the trap this issue warned about

`scripts/check-cmake-image-policy.py`: a tracked cmake file that links
`NanoRos::NanoRos*` into an executable must CALL `nros_apply_panic_policy`
(directly, or via `nano_ros_entry` / `nano_ros_add_executable`, which do it for
their callers).

It keys on a CALL, never a name, because this issue recorded the failure
first-hand: a mechanical grep for "goes through the shared verb" excluded the
ESP-IDF shim because a COMMENT there mentioned `nano_ros_entry()`. So comments
are stripped before anything is matched and the pattern is `name(`. The
self-test carries that exact case — a file whose only mention of the verb is in
a comment MUST still be flagged.

It reads `git ls-files` rather than walking: build trees and the scratch `tmp/`
carry generated CMakeLists that are not the project's to fix, and a gate that
reports them teaches people to skim its output. (Found immediately — the first
run flagged a `tmp/phase-150-smoke-*` scratch tree.)

`check-image-panic-policy.py` is the Rust-side sibling and states that it cannot
see "the C/C++ side, where the policy is a cargo feature on the staticlib". This
is that side; the two are now the halves of one question.

### Verified

* Gate self-test: 6 cases, both directions, including the comment trap.
* MUTATION-checked: deleting the call from the smoke fixture makes the gate fail
  and name that file; restoring it passes. A gate nobody has watched fail has
  unknown discriminating power.
* `cmake_add_subdirectory_smoke` configures AND links clean with the change
  (`Linking C executable smoke`), so the added call is not merely accepted by
  the parser.
* Wired into `check-fast`, beside its Rust-side sibling.

