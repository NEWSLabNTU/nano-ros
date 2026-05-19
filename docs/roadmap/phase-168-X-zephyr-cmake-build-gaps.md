# Phase 168.X — Zephyr CMake build gaps (DDS C-API + C++ logging + register stubs)

**Goal.** Close pre-existing main-branch gaps in the Zephyr CMake
build path so the Phase 168.4 collapsed C / C++ examples build with
every RMW backend their `prj-<rmw>.conf` overlay declares.

**Status.** Not Started — discovered during Phase 168.4 build
verification on `phase-118-example-matrix-collapse`.

**Priority.** P2 — gates the last ~20 collapsed binaries
(`examples/zephyr/{c,cpp}/<case>/` × DDS, plus `examples/zephyr/cpp/<case>/`
× zenoh+xrce). The collapsed example *shape* is complete and matches
the Rust collapse pattern; only the build wiring is blocked.

**Depends on.** Phase 168 (Zephyr Rust collapse + cargo-features
patch + per-example `EXTRA_CARGO_ARGS` mechanism) — fully landed.

---

## Gap 1 — DDS C/C++ on Zephyr: `nros-rmw-dds-staticlib` lacks
`global_allocator` + `panic_handler`

**Symptom.**

```
error: no global memory allocator found but one is required;
       link to std or add `#[global_allocator]` to a static item
       that implements the GlobalAlloc trait
error: `#[panic_handler]` function required, but not found
error: could not compile `nros-rmw-dds-staticlib`
```

Surfaces when the Zephyr module's `CONFIG_NROS_C_API` /
`CONFIG_NROS_CPP_API` branch attempts to link
`libnros_rmw_dds_staticlib.a` to satisfy the Phase 160.A strong
stub's reference to `nros_rmw_dds_register`. The DDS path takes
the staticlib (rather than including the backend as a Cargo dep of
`nros-c` / `nros-cpp`) because Phase 134.fix retired the direct
dep for the same reasons it retired the zenoh dep.

**Why zenoh works.** `nros-rmw-zenoh-staticlib`'s `platform-zephyr`
feature transitively activates `nros-platform/global-allocator`
+ `panic-halt` (paired with the existing FreeRTOS / NuttX / ThreadX
overlays in its `Cargo.toml`). DDS staticlib only forwards the
underlying `nros-rmw-dds/platform-zephyr` feature; it has no
`nros-platform` direct dep, so no allocator / panic glue lands.

**Fix sketch.**

```toml
# packages/dds/nros-rmw-dds-staticlib/Cargo.toml
platform-zephyr = [
    "nros-rmw-dds/platform-zephyr",
    "nros-platform/platform-zephyr",
    "nros-platform/global-allocator",
    "dep:panic-halt",
]
```

with matching `[dependencies]` additions for `nros-platform`
+ `panic-halt`. (The `platform-bare-metal` feature stays
untouched — `nros-platform` doesn't expose a bare-metal feature.)
Mirror the FreeRTOS / NuttX / ThreadX overlays.

## Gap 2 — C++ Zephyr build missing `nros_log_emit` + log_fmt glue

**Symptom.**

```
…/build-cpp-<case>-<rmw>/zephyr.elf: in function `…NROS_LOG_INFO(…)`:
undefined reference to `nros_log_emit_fmt`
```

then, once `log_fmt.c` is pulled into the cpp build:

```
(.text.nros_log_emit_fmt+0x10d): undefined reference to `nros_log_emit`
```

`NROS_LOG_INFO` (et al) macro from `<nros/log.h>` expands to a call
into `nros_log_emit_fmt`, defined in `packages/core/nros-c/c-stubs/
log_fmt.c`. That `printf`-style helper in turn calls
`nros_log_emit`, the Rust impl in `packages/core/nros-c/src/log.rs`.
Both symbols ship with `libnros_c.a`; neither is in
`libnros_cpp.a`.

On the C-API Zephyr path everything works because
`nros_cargo_build(PACKAGE nros-c)` builds the static lib that
carries both symbols, and `zephyr_library_link_libraries(nros_c_cargo)`
pulls them in.

On the C++-only Zephyr path (`CONFIG_NROS_C_API=n,
CONFIG_NROS_CPP_API=y`) `nros-c` is not built — so neither symbol
is available, even though every C++ example uses `NROS_LOG_INFO`.

**Fix sketch.**

Option A — extend `nros-cpp`'s build script to compile both
`c-stubs/log_fmt.c` AND export the `nros_log_emit` Rust impl
(reachable via `pub use nros_c::log::nros_log_emit` or a duplicate
`#[no_mangle]` shim in `nros-cpp/src/log.rs`).

Option B — make the C++ Zephyr branch in `zephyr/CMakeLists.txt`
build `nros-c` alongside `nros-cpp` (with `_nros_features =
"rmw-cffi,platform-zephyr,ros-humble[,std]"`) so the existing C
staticlib provides both symbols. Side-effect: doubles the rlib
closure compile time.

## Gap 3 — DDS C-API + C++ also need the same backend staticlib
emission already in place for zenoh

Once gap 1 lifts, `zephyr/CMakeLists.txt` needs to re-enable the
`nros_cargo_build(PACKAGE nros-rmw-dds-staticlib)` branch that the
168.4 commit stubbed out for both the C-API and C++ paths (mirror
of the zenoh-staticlib emission added there). The cleanup pointer
sits in the same file directly above the `endif()` for
`if(CONFIG_NROS_C_API)` and inside the CPP `if(CONFIG_NROS_RMW_ZENOH)`
block.

---

## Acceptance criteria

- [ ] `examples/zephyr/c/<case>/` builds with `-DCONF_FILE="prj.conf;prj-dds.conf"`.
- [ ] `examples/zephyr/cpp/<case>/` builds with `-DCONF_FILE="prj.conf;prj-<rmw>.conf"` for every `rmw ∈ {zenoh, dds, xrce}`.
- [ ] `examples/zephyr/cpp/cyclonedds/talker-aemv8r/` still builds (cyclonedds path untouched by this work).
- [ ] No regression on the Phase 168.3 Rust collapse (20/20 binaries).
- [ ] No regression on the legacy `examples/zephyr/{c,cpp}/<rmw>/<case>/` builds that exist on `main`.

## Files (when 168.X lands)

- `packages/dds/nros-rmw-dds-staticlib/Cargo.toml` — extend
  `platform-zephyr` to pull `nros-platform/global-allocator` +
  `dep:panic-halt`.
- `packages/core/nros-cpp/build.rs` — either compile the log
  glue (option A) OR no change (option B).
- `zephyr/CMakeLists.txt` — re-enable `nros-rmw-dds-staticlib`
  build in both `CONFIG_NROS_C_API` and `CONFIG_NROS_CPP_API`
  branches; if option B for gap 2, also build `nros-c` from the
  CPP branch.
- `just/zephyr.just` — extend `build-fixtures` to drive the
  collapsed C / C++ cases × each RMW their overlay declares.

## Notes

- The legacy `examples/zephyr/c/{zenoh,dds}/<case>/` builds on
  `main` share the same `nros_rmw_<x>_register` linker gap that
  Phase 168.4 partially fixed for zenoh by adding the
  staticlib build to the module's CMakeLists.txt. Pre-collapse
  CI never exercised the legacy C zenoh build, so the gap was
  never reported.
- The Phase 168.3 Rust collapse is unaffected by these gaps —
  the Rust examples link `nros-rmw-zenoh` / `nros-rmw-dds` /
  `nros-rmw-xrce-cffi` directly via Cargo, so register +
  log + allocator symbols are all bundled inside `librustapp.a`.
