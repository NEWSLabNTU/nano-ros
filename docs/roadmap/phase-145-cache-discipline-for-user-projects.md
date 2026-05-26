# Phase 145 — Cache Discipline for C/C++ User Projects (radar)

**Goal.** Close the residual stale-cache classes that Phase 140's
`install-local` rip-off does NOT close. Phase 140 gives users **one
canonical consumption path** (source-vendored `add_subdirectory`);
Phase 145 gives them **deterministic incremental rebuilds across
NANO_ROS_PLATFORM / NANO_ROS_RMW toggles and submodule bumps**.

**Status.** On radar. Not started. Target audience is the 5% of
users who toggle platforms / RMWs across builds (contributors, CI
matrices, kernel hackers); 95% of single-platform users will never
hit the residual classes.

**Priority.** P3 — quality-of-life for power users, not a
correctness gate. Hold until a real user reports cache drift in
the wild.

**Depends on.** Phase 137/138/139/140 (the source-distribution
shape this phase further hardens).

**Related.** Phase 133.5 (zpico-sys race fix that informs the
approach), Phase 134 (the canonical example of "two paths drifted
because cache invalidation was wrong"), Phase 123.A.7
(`NANO_ROS_GEN_CACHE_DIR` codegen cache — the existing precedent
for explicit cache discipline).

---

## Overview

Even after Phase 140 unifies consumption around `add_subdirectory`,
several stale-cache classes persist for users that change build
configuration between invocations:

| Class | Symptom | Today's mitigation |
|-------|---------|--------------------|
| **cargo incremental cache in user's build dir** | toggling `NANO_ROS_RMW` rebuilds nros-c but reuses cached zpico-sys objects compiled against the old RMW | manual `cargo clean -p zpico-sys` |
| **Per-platform Rust target dirs accumulate** | switching `NANO_ROS_PLATFORM` posix→freertos leaves the posix target dir intact (~500 MB-1 GB each) | manual `rm -rf <build>/cargo/` |
| **CMake reconfigure detection limits** | corrosion's cargo state inside `<build>/cargo/` is invisible to cmake's depfile-based stale-detection | Phase 134 added `_cmake-cargo-stale-guard` for the in-tree build; user side has no equivalent |
| **cbindgen-generated headers** | bumping cbindgen config without source change → stale header (Phase 133.5 race fix handles N readers but not the cbindgen-config-bump case) | atomic same-content rename (133.5 partial fix) |
| **zpico-sys vendored zenoh-pico** | new `c/include/<header>.h` added to `c/include/` after a submodule bump → build.rs's `rerun-if-changed` list may miss it → stale build | Phase 134's drift gate caught the multicast case post-hoc |
| **Codegen output at `<example>/generated/*`** | gitignored, regenerated when cmake-time codegen re-runs — correct WHEN cmake re-runs (same limit as above) | `nano_ros_generate_interfaces` regenerates per-cmake-configure |
| **Per-example `build/` dirs from pre-140 era** | one-time cleanup; old `find_package`-built artefacts confuse the new `add_subdirectory` build | manual `find examples -path '*/build' -type d -exec rm -rf {} +` |

Phase 145 lands a user-facing CMake helper + a per-class cache-key
audit, plus documentation explaining the 5% case.

---

## Architecture

### A. User-facing `nano_ros_invalidate_stale_cache(target)` function

Mirror Phase 134's internal `_cmake-cargo-stale-guard` recipe but
expose it as a function in `cmake/NanoRosGenerateInterfaces.cmake`
(or a sibling `NanoRosCacheDiscipline.cmake`):

```cmake
# User's CMakeLists.txt
add_subdirectory(third_party/nano-ros nano_ros)
add_executable(my_app src/main.c)
target_link_libraries(my_app PRIVATE NanoRos::NanoRos)
nano_ros_invalidate_stale_cache(my_app)   # ← Phase 145
```

The function hashes (a) the active `NANO_ROS_PLATFORM` +
`NANO_ROS_RMW` cache vars, (b) the nano-ros submodule SHA, (c)
contents of `cmake/platform/nano-ros-${NANO_ROS_PLATFORM}.cmake`,
into a stamp file under `<CMAKE_BINARY_DIR>/.nros-cache-stamp`. On
each cmake reconfigure, if the stamp doesn't match, the function
deletes the corrosion cargo state at
`<CMAKE_BINARY_DIR>/_deps/corrosion-*/cargo` (or wherever the
build dir lives) so the next build is from-scratch.

Cost: one extra cargo from-scratch build per cache-var change.
Benefit: zero chance of stale RMW / platform objects shipping into
the final binary.

### B. Per-platform target-dir hygiene

Switching `NANO_ROS_PLATFORM` from posix to freertos today leaves
the posix `target/release/` intact. Two options:

1. **Prune on switch.** When 145's stamp detects a platform
   change, also delete the previous platform's `target/release/`
   subdir. Aggressive — costs ~5 min of rebuild next time the
   user switches back. Disk-friendly.
2. **Per-platform target-dir.** Set `CARGO_TARGET_DIR` to a
   per-platform path so each platform keeps its own cache.
   Wasteful (~1 GB per platform) but switching is fast. Aligns
   with how `examples/native/rust/listener/target-{safety,tls,zero-copy}/`
   already partitions Rust target dirs by feature variant.

Recommend (2) — per-platform target dir under
`<build>/cargo-<plat>-<rmw>/`. Cost is disk; benefit is sub-second
switches.

### C. cbindgen-config-bump gate

Phase 133.5 atomicises the WRITE side. The READ side still trusts
`rerun-if-changed` on the source files cbindgen reads. If the
cbindgen.toml itself changes, the source files don't — so cargo
doesn't re-run build.rs, and the generated header is stale.

Fix: add `cargo:rerun-if-changed=cbindgen.toml` (verify it's there
already — Phase 134 audit work). Plus add the build.rs's own SHA
to the regen condition: if build.rs itself changes (e.g.
cbindgen-config-bump landed in build.rs), force regen.

### D. zpico-sys vendored source list drift

Phase 134.5 was supposed to land a build-time check that globs
`zenoh-pico/src/system/<plat>/**/*.c` and asserts the build.rs source
list matches. Verify status — if 134.5 hasn't landed, Phase 145.4
ships the gate.

### E. Per-example pre-140 `build/` dir cleanup script

One-time housekeeping; not automated. Ship a `just cleanup-stale-examples`
recipe that runs `find examples -path '*/build' -type d -exec rm
-rf {} +`. Document in CLAUDE.md. Self-service for users that
migrate from a pre-140 checkout.

---

## Work Items (all unstarted)

- [ ] **145.1 — `nano_ros_invalidate_stale_cache()` function.**
      Add `cmake/NanoRosCacheDiscipline.cmake`. Include from root
      `CMakeLists.txt`. Document in `book/src/internals/cache-discipline.md`.
      **Files.** `cmake/NanoRosCacheDiscipline.cmake` (new),
      `CMakeLists.txt`, `book/src/internals/cache-discipline.md` (new).

- [ ] **145.2 — Per-platform CARGO_TARGET_DIR.**
      Wire the Phase 138 platform modules to set `CARGO_TARGET_DIR`
      to `<build>/cargo-<plat>-<rmw>/` before any corrosion target
      gets imported. Verify cross-platform builds don't collide.
      **Files.** `cmake/platform/nano-ros-*.cmake`.

- [ ] **145.3 — cbindgen + build.rs SHA gate.**
      Audit every nano-ros `build.rs` that runs cbindgen
      (zpico-sys, nros-c, nros-cpp). Add `cargo:rerun-if-changed`
      for cbindgen.toml + the build.rs itself.
      **Files.** `packages/**/build.rs`.

- [x] **145.4 — Verify the source-list drift gate landed and extend if not.**
      DONE 2026-05-26. **Verified:** the gate landed in `zpico-sys/build.rs`
      (Phase 136.6 — every `zenoh_platforms.toml` `include` root is checked to
      resolve to a real dir with `.c` files, panics on drift, + per-dir
      `rerun-if-changed`; backed by `tests/zpico_drift_gate.rs`). The doc's
      `nros-rmw-zenoh-staticlib` has no build.rs (no vendored list), and
      `nros-rmw-dds-staticlib` no longer exists (dust-dds retired Phase 169).
      `nros-c/build.rs` vendors no external source list (compiles its own
      `src/` + has `rerun-if-changed`). **Extended:** the one remaining
      vendored-C build.rs — `nros-rmw-xrce-cffi` (uxr / micro-cdr submodule
      sources) — had `rerun-if-changed` but no presence check, so a missing
      submodule / upstream layout bump surfaced as a confusing cc-rs
      "file not found". Added a mirror of the 136.6 gate: verify the three
      vendored roots (`micro-xrce-dds-client/src/c`, `micro-cdr/src/c`,
      `nros-rmw-xrce/src`) resolve to dirs with sources, panic up front with a
      `git submodule update --init` hint, + `rerun-if-changed`. Verified: xrce
      stack recompiles clean (gate passes with submodules present).
      **Files.** `packages/xrce/nros-rmw-xrce-cffi/build.rs` (gate added);
      `packages/zpico/zpico-sys/build.rs` (verified, already present).

- [ ] **145.5 — `just cleanup-stale-examples` recipe.**
      One-shot housekeeping for pre-140 users.
      **Files.** `justfile`, `CLAUDE.md`.

- [ ] **145.6 — Doc page.**
      `book/src/internals/cache-discipline.md` — explains the
      classes, the helpers, the 5% audience. Cross-link from
      `book/src/SUMMARY.md` under Internals.
      **Files.** `book/src/internals/cache-discipline.md` (new),
      `book/src/SUMMARY.md`.

---

## Acceptance

- [ ] User project that toggles `NANO_ROS_PLATFORM` posix→freertos
      and back twice produces byte-identical final binaries (no
      stale-cache-derived drift) without any manual `cargo clean`.
- [ ] `book/src/internals/cache-discipline.md` enumerates every
      class + its mitigation.
- [ ] `just cleanup-stale-examples` deletes pre-140 `build/` dirs
      idempotently.
- [ ] No regression in single-platform single-RMW build time
      (95% case shouldn't pay for the 5%-case discipline).

---

## Notes

- **Why P3.** None of these classes cause WRONG output today — they
  cause SLOW rebuilds, occasionally CONFUSED users debugging
  "wait, why is my old RMW still linked". Real, but not
  data-corrupting. Hold until a user reports it.
- **Phase 140 vs Phase 145 boundary.** Phase 140 closes "two paths
  drift" by ripping one path. Phase 145 closes "one path's
  incremental cache drifts" by adding discipline within the
  surviving path. Sequencing matters: Phase 140 first; Phase 145
  builds on its single-path shape.
- **Why not auto-prune on every reconfigure.** Tempting but
  wasteful — a contributor running `cmake -G Ninja` on the same
  config would pay the from-scratch cost every invocation. The
  stamp file approach pays only on actual config change.
- **Per-platform CARGO_TARGET_DIR vs prune-on-switch.** Disk is
  cheaper than wall-clock. ~1 GB per platform × 5 platforms = 5 GB.
  Most user laptops have headroom. CI runners can override the
  default to share a single dir.
- **What this phase explicitly does NOT do.** Doesn't change the
  consumption shape (still `add_subdirectory`). Doesn't add a new
  consumption path (would re-introduce Phase 134-style drift).
  Doesn't replace cargo's incremental cache (we depend on cargo;
  we just gate its inputs).
