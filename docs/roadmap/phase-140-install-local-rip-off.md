# Phase 140 — Rip Off `install-local`

**Goal.** Delete `just install-local` and every recipe that depends on it. Delete the `build/install/` layout (`lib/cmake/NanoRos*`, `lib/libnros_*.a`, `share/nano-ros/`). Delete the `find_package(NanoRos CONFIG)` consumption path. Migrate every internal test fixture, every example, every CI step to the source-distribution path landed in Phase 137 / 138 / 139.

The install-then-link model came from a Debian-shaped vision that doesn't fit how anyone actually consumes nano-ros. Keeping both consumption paths during transition (137–139) costs ongoing maintenance + confuses contributors. Phase 140 pulls the plug.

**Status.** Not started.

**Priority.** P2 — finishes the source-distribution direction. Until 140 lands, `install-local` is documented as legacy and still works but is no longer the recommended path.

**Depends on.** Phase 137 (root entry CMake — required user-facing replacement), Phase 138 (per-platform modules — required for examples), Phase 139 (per-RTOS shells — required for RTOS workflows). Cannot start until all three deliver functional replacements.

**Related.** Phase 135 (`test-all install-local` dep — Phase 140 removes the dep entirely since tests switch to add_subdirectory). Phase 134 (UDP multicast gate — would have been a non-issue under add_subdirectory since `--allow-multiple-definition` is no longer needed; doc the cleanup).

---

## Overview

`install-local` was added to satisfy the `find_package(NanoRos CONFIG)` consumption shape. That shape made sense when nano-ros was imagined as a Debian-style installed library. Three observations made the model wrong:

1. **RTOS users don't `find_package`.** Zephyr, ESP-IDF, PlatformIO, NuttX, PX4 — all consume via in-tree source. `install-local` was always an awkward bolt-on for them.
2. **`install-local` is heavy.** ~30 s warm, ~10 min cold. Required before every fresh `just ci`. Phase 135 added it as a `test-all` dep to fix first-run breakage; the dep itself was a smell.
3. **Two consumption paths drift.** Phase 134's UDP multicast linker error existed because the install path and the cargo-rust-only path used different archives with different symbol contents. Single path = single failure mode.

After Phase 137 + 138 + 139, every consumer (internal test fixture, in-tree example, external RTOS user) goes through `add_subdirectory(nano-ros)` or one of its RTOS-shaped re-exports. The install path has no remaining consumers. Phase 140 deletes it.

---

## Architecture

### A. What gets deleted

```
justfile:
  - recipe `install-local`                          ← deleted
  - recipe `install-local-posix`                    ← deleted
  - recipe `install-platform-posix`                 ← deleted
  - recipe `install-rmw-zenoh`                      ← deleted
  - recipe `install-rmw-dds`                        ← deleted
  - dep `install-local` on `test-all`               ← removed (Phase 135.1)

just/freertos.just:
  - recipe `install-platform`, `install`            ← deleted

just/nuttx.just:
  - recipe `install`                                ← deleted

just/threadx_linux.just, threadx_riscv64.just:
  - recipe `install-platform`, `install`            ← deleted

tests/c-msg-gen-tests.sh:
  - `just install-local` call (line 56)             ← replaced with add_subdirectory smoke

build/install/                                      ← directory removed from .gitignore + repo
```

### B. CMake files deleted

```
packages/core/nros-c/CMakeLists.txt:
  - install(...) rules for libnros_c.a + NanoRosCConfig.cmake     ← deleted

packages/core/nros-cpp/CMakeLists.txt:
  - install(...) rules for NanoRosCppConfig.cmake                 ← deleted

packages/core/nros-platform-*/CMakeLists.txt:
  - install(...) rules + Config.cmake.in files                    ← deleted

packages/zpico/nros-rmw-zenoh-staticlib/CMakeLists.txt:
  - install(...) rules + NrosRmwZenohConfig.cmake.in              ← deleted

packages/dds/nros-rmw-dds-staticlib/CMakeLists.txt:
  - similar                                                       ← deleted

packages/xrce/nros-rmw-xrce-cffi/CMakeLists.txt:
  - similar                                                       ← deleted

packages/dds/nros-rmw-cyclonedds/CMakeLists.txt:
  - install rules                                                 ← deleted

cmake/install.cmake                                               ← deleted (Phase 137 left this for transition)
```

### C. Test-fixture migration

Today `packages/testing/nros-tests/src/fixtures/binaries/mod.rs::build_c_example` invokes:

```rust
cmd!("cmake", "-DCMAKE_PREFIX_PATH=…/build/install", "..")
```

Post-140 it invokes:

```rust
cmd!("cmake", "-DNANO_ROS_PLATFORM=posix", "-DNANO_ROS_RMW=zenoh", "..")
```

The example's `CMakeLists.txt` already uses `add_subdirectory` (Phase 138.4). The fixture stops setting `CMAKE_PREFIX_PATH`.

### D. Doc migration

- `book/src/getting-started/installation.md` — rewritten as
  `getting-started/build-as-subdirectory.md` (the page Phase 137.6
  already added). Old page deleted (or kept as a 301-redirect stub
  for one release cycle).
- `docs/reference/c-api-cmake.md` — updated: every `find_package(NanoRos)`
  example becomes `add_subdirectory(third_party/nano-ros)`.
- `CLAUDE.md` "### CMake Path Convention" — section already covers
  the no-walk-up rule; remove the historical "drivers pass absolute
  paths to install prefix" wording, replace with "drivers set
  NANO_ROS_PLATFORM/RMW cache vars before add_subdirectory".

---

## Work Items

- [ ] **140.1 — Audit remaining consumers of `install-local`.**
      `git grep -l 'install-local\|build/install\|find_package(NanoRos'`.
      Build a table: file → consumption shape → required Phase 137/138/139
      replacement. Land table in this doc under "Notes" before deleting.
      **Files.** none (read-only audit).

- [ ] **140.2 — Migrate `nros-tests` fixtures.**
      `packages/testing/nros-tests/src/fixtures/binaries/mod.rs`:
      stop passing `CMAKE_PREFIX_PATH=…/build/install`. Set
      `NANO_ROS_PLATFORM` + `NANO_ROS_RMW` cache vars instead. Each
      `build_c_example` / `build_cpp_example` invocation flows through
      the per-example `add_subdirectory` path.
      **Files.** `packages/testing/nros-tests/src/fixtures/binaries/mod.rs`.

- [ ] **140.3 — Migrate `c-msg-gen-tests.sh`.**
      `tests/c-msg-gen-tests.sh:56` calls `just install-local`. Replace
      with an `add_subdirectory`-shaped smoke build of the codegen
      output (or move the test into `packages/testing/nros-tests/` as a
      Rust test using the new fixture).
      **Files.** `tests/c-msg-gen-tests.sh`.

- [ ] **140.4 — Delete install recipes.**
      `justfile` + `just/{freertos,nuttx,threadx_linux,threadx_riscv64}.just`:
      delete every install-* recipe. Remove dep on `install-local` from
      `test-all` (Phase 135.1 added it as a transitional dep; 140 drops it).
      **Files.** `justfile`, `just/*.just`.

- [ ] **140.5 — Delete CMake install rules.**
      Every `install(TARGETS …)`, `install(FILES …)`,
      `configure_package_config_file(…)`, `write_basic_package_version_file(…)`
      under `packages/` is deleted. The `Config.cmake.in` templates
      under `packages/*/cmake/` are deleted too.
      **Files.** per the §B list.

- [ ] **140.6 — Delete the install-mode branch from root CMake.**
      Phase 137's root `CMakeLists.txt` gated install rules on
      `PROJECT_IS_TOP_LEVEL`. After 140 there are no install rules at
      all — delete the branch and the `cmake/install.cmake` file.
      **Files.** `CMakeLists.txt`, `cmake/install.cmake` (deleted).

- [ ] **140.7 — Delete `build/install/` artefacts + ignore patterns.**
      `rm -rf build/install/`. Remove `/build/install/` from
      `.gitignore` (not needed once the dir is no longer produced).
      **Files.** `.gitignore`.

- [ ] **140.8 — Doc migration.**
      `book/src/getting-started/installation.md` → rewrite or delete
      per §D. `docs/reference/c-api-cmake.md` updated.
      `CLAUDE.md` wording updated. Audit `book/src/` + `docs/` for
      any remaining `find_package(NanoRos)` or `build/install/` refs.
      **Files.** `book/src/getting-started/installation.md`,
      `docs/reference/c-api-cmake.md`, `CLAUDE.md`, and others per audit.

- [ ] **140.9 — Migration note for downstream.**
      `docs/release/migration-install-local-removal.md` — one page
      explaining the breaking change, the before / after invocation,
      pointers to Phase 137 + 139 entry docs. Linked from
      `book/src/SUMMARY.md` under Release Notes.
      **Files.** `docs/release/migration-install-local-removal.md` (new),
      `book/src/SUMMARY.md`.

- [ ] **140.10 — Verify clean.**
      `git grep -l 'install-local\|build/install\|find_package(NanoRos\|NanoRosConfig'`
      returns nothing (besides this doc + the migration note from 140.9).
      `just ci` green end-to-end with no install step run.
      **Files.** none (verification).

---

## Acceptance

- [ ] `just install-local` recipe does not exist; running it fails
      with "no such recipe".
- [ ] `find_package(NanoRos)` has no callers anywhere in the repo.
- [ ] `build/install/` directory is never created by any build path.
- [ ] First-run cold `just ci` from a fresh `git clone` passes
      end-to-end with no install step — driven by `add_subdirectory`
      + per-RTOS integrations from Phase 137 / 138 / 139.
- [ ] All examples + tests + RTOS integrations build via the
      source-distribution path. No fixture sets `CMAKE_PREFIX_PATH`.
- [ ] `git grep` audit from 140.10 returns clean.
- [ ] Migration note from 140.9 published; downstream consumers have
      a clear migration path.

---

## Notes

- **Why a clean rip, not a slow deprecation.** Two paths drift
  (Phase 134 was the proof). One path is what we ship. Keeping
  `find_package(NanoRos)` around as "legacy supported" makes future
  contributors guess which path is canonical. The rip-off + the
  migration note are clearer than a long deprecation window.
- **What `install-local` got right.** Codegen caching
  (`NANO_ROS_GEN_CACHE_DIR`, Phase 123.A.7) and the `Layer-2`
  per-RTOS CMake helpers are real abstractions. They survive intact
  under the source-distribution model — codegen cache is
  environment-driven (orthogonal to consumption shape), Layer-2
  helpers move to `cmake/platform/` in Phase 138. Phase 140 doesn't
  delete the good ideas; it deletes the wrapping that forced them
  through an install prefix.
- **`colcon-nano-ros` impact.** The colcon plugin lives in
  `packages/codegen/` (submodule). It currently runs `cargo
  nano-ros generate-rust` per package and trusts a separate install
  prefix on the side. Post-140 it points at the in-source codegen
  cache directly — small change, tracked in colcon-nano-ros's own
  repo, not blocking 140.
- **Distro packaging (Debian, Fedora, ROS overlay).** Out of scope
  for 140. If a maintainer wants a Debian package later, they ship
  a `.deb` that vendors the source tree at `/usr/src/nano-ros/`
  and a `dpkg-buildpackage` shim. That's a downstream task, not a
  reason to keep `install-local` in-tree.
