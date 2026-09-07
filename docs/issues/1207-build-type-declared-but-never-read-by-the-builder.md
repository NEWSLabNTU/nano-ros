---
id: 1207
title: "`nros build` re-derives each package's build path from `CMakeLists.txt` presence at three sites and never reads `<build_type>` — the field 411 package.xml declare says exactly that, and colcon routes on it"
status: open
area: [cli, build]
severity: medium
related: [phase-420, phase-383, "RFC-0087", "RFC-0065"]
---

# One fact, two derivations, no gate between them

RFC-0087 D2 defines `<build_type>` as the answer to "**which build system
builds this package**", and phase-420 W3 rewrote 371 `package.xml` to say it.
`build_type.rs` lowers every spelling to a `BuildPath::{Cargo, Cmake}`
(`packages/cli/nros-cli-core/src/build_type.rs:52-81`), and the cmake reader
mirrors the same table (`cmake/NanoRosPackageXml.cmake:86-101`), cross-checked
by `scripts/check-build-type-spelling.py` so the two tables cannot disagree.

**`nros build` consults neither.** Every point in the pipeline that needs the
same answer computes it from the presence of a file on disk:

| site | code | question answered |
| --- | --- | --- |
| `cmd/build.rs:263-266` | `.filter(\|p\| p.dir.join("CMakeLists.txt").is_file())` → `ws_non_rust` → `image_has_non_rust` → `plan::driver_for` | which DRIVER (cargo vs cmake) builds the image |
| `builder/cargo_root.rs:87` | `if !pkg.dir.join("Cargo.toml").is_file() { continue; }` | is this a `[workspace] members` entry |
| `builder/cmake_root.rs:120` | `if … !pkg.dir.join("CMakeLists.txt").is_file() { continue; }` | is this an `add_subdirectory()` |

`PackageXml` — the struct the discovery walk actually produces
(`packages/cli/cargo-nano-ros/src/package_xml.rs:47-71`) — has **no
`build_type` field at all**. `<build_type>` is parsed only by two hand-rolled
string scanners outside that path: `prereq_resolve::build_type`
(`orchestration/prereq_resolve.rs:219-226`, a `str::find` on the tag) and the
CMake regex in `nano_ros_read_package_export`
(`cmake/NanoRosPackageXml.cmake:207-219`).

## Nothing reads the value the CMake reader produces

`NANO_ROS_EXPORT_BUILD_TYPE` / `_RAW` are set into the parent scope
(`cmake/NanoRosPackageXml.cmake:217-219`) and **grepping the tree finds zero
readers** outside the file that sets them. The reader, its canonicalisation and
its retirement warning are exercised by a `cmake -P` gate and by nothing else.

## The live consumers, and what they disagree about

`<build_type>` is not inert — it has three real readers, and none is the
builder:

1. **`nros setup --workspace`** (`cmd/setup.rs:2922-2965`, phase-435 W2) counts
   `<build_type>` values and provisions the host tools each implies, from
   `nros-sdk-index.toml`:
   `[build_type.nros_cargo] packages = ["cargo"]`,
   `[build_type.nros_cmake] packages = ["cmake", "clang"]`
   (`nros-sdk-index.toml:56-64`).
2. **The colcon extension** (`colcon_nano_ros/manifest.py`, phase-420 W4)
   selects the cargo or the cmake build task **from `<build_type>`** — its
   entry points are `ros.nros_cargo` / `ros.nros_cmake`.
3. **`check-build-type-spelling`**, whose four rules are `duplicate-build-type`,
   `unknown-spelling`, `owned-declares-ament` / `owned-declares-nothing`, and
   `interface-declares-nros` (`scripts/check-build-type-spelling.py:260-284`).

So the same package is routed **by declaration** under `colcon build` and **by
file presence** under `nros build`, and the gate checks the *class* boundary
(ament vs nros, interface vs owned) — never that `_cargo`/`_cmake` matches what
is on disk.

## Measured today: no live disagreement, and that is the whole risk

Cross-tabulated over all 416 in-tree `package.xml` (411 declare a build type):

```
163  nros_cmake   CMakeLists.txt only
151  nros_cargo   Cargo.toml only
 23  nros_cargo   NEITHER file
 22  nros_cmake   NEITHER file
 21  nros_cmake   BOTH  (the Zephyr / ThreadX Rust leaves — cmake root, cargo staticlib)
 12  ament_cmake  NEITHER
  7  ament_cmake  CMakeLists.txt only
  7  ament_cargo  Cargo.toml only
  3  cmake        CMakeLists.txt only
  2  ament_cargo  NEITHER
```

**Zero cargo-declared packages carry a `CMakeLists.txt`.** The two derivations
agree everywhere — which is expected, because phase-420 W3 assigned the values
*using the same evidence*: its own note records the rule as "otherwise
`CMakeLists.txt` before `Cargo.toml`". The declaration was seeded from the file
presence it is now supposed to be authoritative over, so agreement today is a
tautology, not a check.

The 45 packages with **neither** file are the interesting ones: bringups (which
own no build file — they generate a root per image) and embedded C/C++ entries
(RFC-0065 measured them at zero source files). For those the declaration is the
*only* statement of build path, and it is exactly there that no consumer reads
it.

## What breaks when they drift

* A package that grows a `CMakeLists.txt` while still declaring `nros_cargo`
  silently flips the image's driver to cmake (`image_has_non_rust` is
  file-based), with no diagnostic naming the declaration it contradicts.
* `nros setup --workspace` provisions from the declaration. A package declaring
  `nros_cargo` that in fact builds through cmake gets `cargo` provisioned and
  not `cmake`/`clang`; the failure arrives later as a missing host tool with no
  link back to the mismatch.
* A `nros_cmake` package with no `CMakeLists.txt` is skipped by
  `cmake_root::render`'s subdir loop with no warning — it is simply absent from
  the image, which is the "a workspace that gains a package but forgets the
  `SUBDIRS` line simply does not build it — silently" failure RFC-0065's
  Problem section exists to end.

## Suggested shape

Either make the builder read `build_type::canonical(...).path` and treat the
file presence as the cross-check, or add the missing gate rule
(`declared-path-disagrees-with-files`: a `*_cargo` package may not carry a
`CMakeLists.txt`; a `*_cmake` package must carry one **or** be a bringup /
declaration-only entry). The second is cheap and would have caught the class
before it can fire; the first is what RFC-0087 D2 actually promises.

Note the 21 `nros_cmake`-with-both packages mean the rule is not "exactly one
file" — a cmake-rooted package importing a crate through corrosion is a normal
and correct shape, and any gate must permit it.

## Evidence

* `packages/cli/nros-cli-core/src/cmd/build.rs:255-298` — `ws_non_rust` /
  `image_has_non_rust`.
* `packages/cli/nros-cli-core/src/builder/plan.rs:113-123` — `driver_for`.
* `packages/cli/nros-cli-core/src/builder/cargo_root.rs:81-92`.
* `packages/cli/nros-cli-core/src/builder/cmake_root.rs:118-152`.
* `packages/cli/cargo-nano-ros/src/package_xml.rs:47-71` — no `build_type`
  field.
* `packages/cli/nros-cli-core/src/build_type.rs:52-113` — the table nobody in
  the build pipeline calls.
* `scripts/check-build-type-spelling.py:260-284` — the four rules, none about
  file agreement.
