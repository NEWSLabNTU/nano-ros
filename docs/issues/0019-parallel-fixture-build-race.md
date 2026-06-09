---
id: 19
title: Parallel Zephyr fixture builds race on shared build-dir / probe writes
status: open
type: tech-debt
area: build
related: [phase-226]
---

Surfaced during the issue #11 #3 RMW-variant verification: building Zephyr
native_sim fixtures with `NROS_ZEPHYR_BUILD_JOBS >= 2` (or with two
overlapping `just zephyr build-fixtures` invocations) intermittently fails a
leaf with garbled, interleaved CMake/ninja/cargo errors, while an **isolated
serial re-run of the same leaf passes clean**.

**Observed symptoms** (all transient, all clear on a `NROS_ZEPHYR_BUILD_JOBS=1`
retry):

- `nros-c` size-probe nested cargo:
  `error: failed to write '.../build/nros-c-*/out/sizes-probe-target-*/release/.fingerprint/...'  No such file or directory (os error 2)`
  → `nros-sizes-build: isolated probe failed; falling back to filesystem
  watch` → `nros-c probe: no rlib matching libnros-*.rlib found`.
- Zephyr core boilerplate, *before* `project()` / `nros_find_interfaces()` is
  reached: `CMakeDetermineASMCompiler.cmake: configure_file ... No such file
  or directory`, `extensions.cmake: file COPY_FILE failed`, and the pinned
  `third-party/ninja -t recompact: loading 'build.ninja': No such file or
  directory`.
- `rm -rf <build-dir>` reporting `Directory not empty` because a cargo
  process from a prior/concurrent job was still writing into it.

**Diagnosis.** These all fire *before* the package.xml/codegen migration code
runs, so they are **not** a `nros_find_interfaces` / message-deps regression —
they are a build-orchestration concurrency hazard: concurrent jobs (and/or a
scheduler cleanup `rm` racing an in-flight cargo) writing the same Zephyr
build directory / cargo target tree. The `nros-c` size-probe nests a cargo
build into `$OUT_DIR/sizes-probe-target-<rustc-slug>/`; under parallel
scheduling its `.fingerprint` writes can collide with a sibling job tearing
down or rebuilding an overlapping path.

**Workaround:** build Zephyr fixtures serially — `NROS_ZEPHYR_BUILD_JOBS=1`
builds clean. Avoid overlapping `just zephyr build-fixtures` invocations on
the same workspace.

**To fix** (one or more of):

- Guarantee per-leaf build-dir / cargo-target isolation in the Zephyr fixture
  make-driver so two concurrent leaves never share a target tree, and ensure
  any cleanup `rm` of a build dir is sequenced *after* that leaf's processes
  exit (`.DELETE_ON_ERROR` / job ordering).
- Make the `nros-c` size-probe robust under concurrency — a per-build unique
  probe target dir (it already supports `NROS_SIZES_PROBE_TARGET_DIR`) and/or
  a lock around the nested cargo, so a colliding `.fingerprint` write cannot
  fail the whole leaf (it already falls back to the filesystem-watch path on
  probe failure, but the leaf still errors).

Cross-ref: Phase 226 (fixture build orchestration) owns the Zephyr scheduler;
the size-probe lives in `packages/core/nros-sizes-build/` (+ `nros-c`/`nros-cpp`
`build.rs`). The maintainer noted this "should be fixed" — tracked here.
