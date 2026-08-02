# RFC-0065 — A colcon-like builder: the workspace root stops being hand-written

**Status:** Draft (2026-08-02)
**Amends:** RFC-0048 (ament/CMake integration + presets), RFC-0026 (examples are
standalone copy-out projects)
**Relates to:** [RFC-0063](0063-system-model-is-a-build-artifact.md) — that RFC
says the resolved model belongs in `build/`; this one decides who *owns* `build/`.

## Problem

A nano-ros workspace user maintains a build-system entry file at the workspace
root — `CMakeLists.txt` for C/C++, `Cargo.toml` for Rust. It is not a thin
shim. `examples/workspaces/c/CMakeLists.txt` is ~70 lines and does four jobs:

```cmake
# 1. board -> toolchain file, BEFORE project() (the first compiler probe)
if(NANO_ROS_BOARD STREQUAL "mps2-an385-freertos")
    set(CMAKE_TOOLCHAIN_FILE ".../cmake/toolchain/arm-freertos-armcm3.cmake")
endif()

# 2. the package list, by hand
set(_ws_subdirs src/c_talker_pkg src/c_listener_pkg)

# 3. which ENTRIES belong to the active platform, by hand
if(NANO_ROS_PLATFORM STREQUAL "posix")
    list(APPEND _ws_subdirs src/native_entry src/native_entry_robot1 …)
elseif(NANO_ROS_BOARD STREQUAL "nuttx-qemu-arm")
    list(APPEND _ws_subdirs src/nuttx_entry)
    # 4. scope workarounds
    set(NUTTX_DIR "$ENV{NUTTX_DIR}" CACHE PATH "NuttX kernel tree")
endif()

nano_ros_workspace(BACKEND zenoh PLATFORM "${NANO_ROS_PLATFORM}"
                   SYSTEM demo_bringup SUBDIRS ${_ws_subdirs})
```

**Every one of those four is derivable, and none of them is user intent.**

- The package list is a `package.xml` tree-walk — colcon's entire job.
- Which entries belong to a platform is already declared in the system config's
  `[deploy.*]` blocks.
- The board→toolchain map already exists as `cmake/toolchain/*.cmake` keyed by
  the board name.
- The `NUTTX_DIR` promotion is a cmake directory-scope workaround, not a
  decision anyone wants to make.

There are **154 tracked `CMakeLists.txt`** under `examples/workspaces/`. Each
copy can drift independently, and a workspace that gains a package but forgets
the `SUBDIRS` line simply does not build it — silently, because an absent
subdir is not an error.

## What we are NOT

Worth stating plainly, because the analogy misleads if taken too far. `colcon
build` walks a package tree and builds each package into its own artifact,
producing `build/` + `install/` with per-package shared libraries that are then
composed at runtime.

nano-ros does something different: it reads the **launch tree** and produces
**one unified binary for a target RTOS**. There is no per-package shared
library, no `install/` to source, no runtime composition. Packages are inputs
to a whole-system bake, not independently deployable units.

So the borrowing is the FRONT of colcon (discover packages by walking the tree;
require no hand-maintained root manifest; put output under `build/`), not the
back.

## Decision

**`nros build` becomes the workspace entry point, and the workspace root needs
no hand-written build file.**

The user declares exactly two things that cannot be derived:

1. **the target** — which RTOS platform / board this build is for;
2. **the entry launch file** — which bringup package's launch tree defines the
   system.

Everything else the builder computes:

| Today, hand-written | Becomes |
| --- | --- |
| `SUBDIRS` package list | `package.xml` tree-walk |
| per-platform entry selection | the system config's `[deploy.*]` blocks |
| board → `CMAKE_TOOLCHAIN_FILE` | board key → `cmake/toolchain/*.cmake` |
| scope/env promotion (`NUTTX_DIR`) | builder-owned |
| `BACKEND` / `PLATFORM` / `EDITION` args | the target declaration |

The discovery half already exists. `nros ws` walks
`<root>/src/<pkg>/package.xml` today and distinguishes colcon-mode from a
single standalone package. This RFC promotes that walk from a msg-sync helper
into the build entry point.

`nano_ros_workspace()` does not disappear — it becomes an artifact the builder
GENERATES into `build/`, not a function users call with hand-maintained
arguments.

## Consequences

**The builder owns `build/`.** RFC-0063 moves the resolved SystemModel there
and leaves the exact layout open; this RFC answers it — `build/` is a builder
output tree, so the model lands in it for the same reason object files do. The
two RFCs should land in that order (0063's inputs work first, since a builder
that regenerates a model is only safe once the model is reproducible).

**`generated/` and `metadata/` follow.** They are already gitignored-in-source
(phase-330 W3.a); once a builder owns `build/`, leaving derived msg crates
under `src/` has no defence. Sequencing matters: the msg-crate redirects in
each leaf's `.cargo/config.toml` are RELATIVE paths, and issue 0378 is the live
reminder that a wrong redirect resolves to a stranger's crate on crates.io.

**Standalone copy-out examples are the hard constraint** (RFC-0026: no
workspace walk-up). A copied-out example has no workspace root to discover
from. Either the builder works on a single-package tree — which `nros ws`
already detects — or copy-out examples keep a generated build file. This is the
requirement most likely to shape the design, so it should be settled first,
not last.

**Fixture build options gain a natural home.** `examples/fixtures.toml`
declares itself the SSoT for per-fixture build options (363 rows) but its own
header records the rollout as incomplete — "native rust = authored + consumed
by the probe … C/C++ cmake cells + cross platforms = rollout in progress". A
builder that takes `(target, bringup)` is exactly what a fixture row already
is, so the row becomes an invocation rather than a description of one.

## Open questions

- **How the per-package build is driven.** Generate a `CMakeLists.txt` into
  `build/` and invoke cmake, drive cmake's file API directly, or synthesize a
  cargo workspace for Rust? Mixed C/C++/Rust workspaces must work under one
  answer.
- **Is there an `install/`?** We produce one binary, so probably not — but
  deploy scripts and the book currently speak colcon's vocabulary.
- **Incremental behaviour.** `build/<pkg>/` per package (colcon-shaped) versus
  one build tree per (target, config). Today's `build-*` / `target-*`
  proliferation is the thing to avoid re-creating.
- **Where the target is declared.** CLI flag, a workspace-level file, or the
  system config. A file re-introduces a root manifest — which is what this RFC
  removes — so a flag plus the bringup path is the default assumption.

## Non-goals

Per-package shared libraries, an `install/` tree to source, or runtime
composition of independently built packages. Those are colcon's model and
explicitly not nano-ros's — the unified-binary bake is the point.
