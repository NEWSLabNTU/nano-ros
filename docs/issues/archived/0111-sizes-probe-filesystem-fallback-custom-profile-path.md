---
id: 111
title: "`nros-sizes-build` filesystem fallback searches `PROFILE` (`release`) not the real profile dir (`nros-fast-release`) → rlib never found"
status: resolved
type: bug
area: core
related: [phase-118]
---

## Resolution

Added `profile_dir_name()` to `packages/core/nros-sizes-build/src/lib.rs`: it derives
the real profile-directory name from `OUT_DIR` (the path component immediately before
`build`), the same walk `cargo_target_dir()` already does. `find_dep_rlib_filesystem`
now uses it in place of the lossy `PROFILE` env var, falling back to `PROFILE` only when
`OUT_DIR` is absent. The fallback now searches `<target>/<triple>/nros-fast-release/deps`
and finds the rlib. Unit test `profile_dir_name_reads_custom_profile_from_out_dir` covers
the custom-profile, host-build, and no-`build`-ancestor cases.

**Verified end-to-end on the affected dev box:** with the isolated probe still SIGSEGVing
(unrelated, see memory `box-fixture-sizes-probe-sigsegv`), the corrected fallback now
locates the rlib — the `EXECUTOR_SIZE probe returned 0` failure is gone (`segv=0`,
`probe-returned-0=0`), and all **Rust + C** zephyr fixtures build (16 `libnros.a` links).
The remaining zephyr **C++** fixture failures are a *separate* pre-existing issue — Zephyr's
minimal C++ library lacks `<string>`, which `component_node.hpp` includes unconditionally —
tracked as #112.

## Summary

`nros-sizes-build`'s **filesystem-watch fallback** (`find_dep_rlib_filesystem`,
`packages/core/nros-sizes-build/src/lib.rs`) builds its search paths from the
`PROFILE` build-script env var:

```rust
let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
// searched: <target>/<triple>/<profile>/deps/  and  <target>/<profile>/deps/
```

But cargo's `PROFILE` env var only ever reports **`debug`** or **`release`** — it does
*not* report a custom profile's name. Custom profiles write their artifacts to a target
subdirectory named after the **profile**, not its `inherits` base. The fixtures build
`nros` under `[profile.nros-fast-release]` (inherits `release`), so:

- real rlib path: `<target>/<triple>/nros-fast-release/deps/libnros-*.rlib`
- fallback searches: `<target>/<triple>/release/deps/` (because `PROFILE == "release"`)

→ **path mismatch**, the fallback never finds the rlib, polls until the 60s timeout,
returns `EXECUTOR_SIZE = 0`, and `nros-cpp` (lib) fails with "1 previous error".

## Impact

The isolated nested-cargo probe is the primary path and normally succeeds, so this latent
fallback bug is invisible on healthy hardware. It bites whenever the isolated probe fails
and the fallback is exercised — e.g. when the nested probe's rustc **SIGSEGVs** under the
heavy `nros-fast-release` fat-LTO compile (observed on one dev box building the zephyr
`nros-cpp` fixture; see agent memory `box-fixture-sizes-probe-sigsegv`). On that box every
zephyr C/C++ fixture fails, blocking `just ci`'s `test-all` + `cyclonedds-ci` entirely,
because the otherwise-correct fallback looks in the wrong directory.

It would also misfire for any deliberate `NROS_SIZES_PROBE_MODE=filesystem` run against a
custom profile.

## Root cause

The correct profile-*directory* name is already derivable from `OUT_DIR`
(`<target>/<triple>?/<profile-dir>/build/<pkg>-<hash>/out`): the path component
immediately before `build` IS the profile dir. `cargo_target_dir()` in the same file
already walks `OUT_DIR` and reads `parent.parent()` for exactly this position — but
`find_dep_rlib_filesystem` ignores it and uses the lossy `PROFILE` env var instead.

## Fix

Add a `profile_dir_name()` helper that extracts the profile-dir component from `OUT_DIR`
(mirroring `cargo_target_dir`'s walk) and use it in `find_dep_rlib_filesystem` in place of
`PROFILE`, falling back to `PROFILE` only when `OUT_DIR` is absent. Unit-test the
derivation against a synthetic custom-profile `OUT_DIR`.
