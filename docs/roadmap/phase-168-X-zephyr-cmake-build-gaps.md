# Phase 168.X — Zephyr collapse: cyclonedds across all languages + cpp log-glue

**Goal.** Close out Phase 168 by:

1. Lifting the C++ Zephyr build path so `examples/zephyr/cpp/<case>/`
   collapses build cleanly with `prj-zenoh.conf` / `prj-xrce.conf`
   / `prj-cyclonedds.conf` overlays.
2. Adding a `cyclonedds` RMW option to **every** collapsed
   language axis (Rust + C + C++), wired through `prj-cyclonedds.conf`
   overlays + matching CMake / Cargo plumbing.
3. Extending the E2E test surface
   (`packages/testing/nros-tests/`) to exercise the cyclonedds path
   end-to-end alongside the existing zenoh + xrce coverage.

**Status.** Not Started — three independent sub-gaps, one
follow-up phase ID.

**Priority.** P2 — gates Phase 168 closure. The 168.3 Rust collapse
(13 binaries × {zenoh, xrce} + sca × zenoh) + 168.4 C collapse
(12 binaries × {zenoh, xrce}) are unaffected and already pass
their smokes; this work strictly adds the cyclonedds row + the C++
language axis.

**Depends on.** Phase 168 (collapse mechanism), Phase 169
(dust-dds retirement; cyclonedds canonical DDS backend),
Phase 117 (cyclonedds C++ Zephyr module wiring).

---

## Gap 1 — C++ Zephyr build missing `nros_log_emit` + `log_fmt` glue

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
into `nros_log_emit_fmt`, defined in
`packages/core/nros-c/c-stubs/log_fmt.c`. That `printf`-style helper
in turn calls `nros_log_emit`, the Rust impl in
`packages/core/nros-c/src/log.rs`. Both symbols ship with
`libnros_c.a`; neither is in `libnros_cpp.a`.

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

## Gap 2 — cyclonedds option on every collapsed language

### 2.A C++ collapsed cases (scaffolded; build gated on Gap 1)

`examples/zephyr/cpp/<case>/prj-cyclonedds.conf` overlays already
exist for all six C++ collapsed cases (talker, listener, ss, sc,
as, ac), each toggling `CONFIG_NROS_RMW_CYCLONEDDS=y` plus the
RTPS/IGMP/POSIX glue Cyclone needs. The cpp `main.cpp` files
include an `#elif defined(CONFIG_NROS_RMW_CYCLONEDDS)` branch that
calls `nros::init("", CONFIG_NROS_DOMAIN_ID)` (mirror of the
existing `examples/zephyr/cpp/cyclonedds/talker-aemv8r/` reference).
Build verification waits on Gap 1.

### 2.B C collapsed cases (Kconfig change required)

Today `zephyr/Kconfig` declares:

```
config NROS_RMW_CYCLONEDDS
    bool "Cyclone DDS (nros-rmw-cyclonedds)"
    depends on NET_SOCKETS && POSIX_API && CPP && NROS_CPP_API
```

The `CPP && NROS_CPP_API` clause locks Cyclone to the C++ API
because `nros_rmw_cyclonedds_register` is declared in C++ today.
To wire Cyclone into the C collapsed cases, either:

- **Option B-1**: drop the `NROS_CPP_API` dep + declare
  `nros_rmw_cyclonedds_register` with C linkage. The standalone
  `packages/dds/nros-rmw-cyclonedds/` already builds with
  `extern "C"` register entry — wrap the declaration in a C-visible
  header so the `nros-c` strong-stub emission in
  `zephyr/CMakeLists.txt :: CONFIG_NROS_C_API` branch resolves it.
- **Option B-2**: declare Cyclone unsupported on the C side and
  drop `prj-cyclonedds.conf` from `examples/zephyr/c/<case>/`.
  This narrows the matrix but keeps the existing constraint.

Option B-1 is the user's explicit ask (cyclonedds on every
language). After landing, copy `prj-cyclonedds.conf` from
`examples/zephyr/cpp/<case>/` → `examples/zephyr/c/<case>/` and
verify with `west build -- -DCONF_FILE="prj.conf;prj-cyclonedds.conf"`.

### 2.C Rust collapsed cases (waits on Phase 169.5)

Rust has no Cyclone DDS backend today. Phase 169.4 deleted
`nros-rmw-dds` (dust-dds) and Phase 169.5 left a future
`nros-rmw-cyclonedds-sys` shim as TBD. When that shim lands:

1. Add `nros-rmw-cyclonedds[-sys] = { ..., optional = true }` to
   each `examples/zephyr/rust/<case>/Cargo.toml`.
2. Add `rmw-cyclonedds = ["dep:nros-rmw-cyclonedds[-sys]"]` feature.
3. Add `nros_rmw_cyclonedds::register()` call to the
   `register_rmw()` helper in `src/lib.rs`, gated by
   `#[cfg(feature = "rmw-cyclonedds")]`.
4. Add `make_config()` variant for cyclonedds (empty locator,
   domain id).
5. Copy `prj-cyclonedds.conf` from cpp scaffold.
6. Wire `EXTRA_CARGO_ARGS=--features rmw-cyclonedds` into the
   `CMakeLists.txt` `elseif(CONFIG_NROS_RMW_CYCLONEDDS)` branch.

## Gap 3 — E2E tests covering cyclonedds

After 1 + 2 land:

- `packages/testing/nros-tests/tests/phase_118_collapse.rs`
  - Extend `test_zephyr_rust_case_rmw_variant_exists` with
    `case::*_cyclonedds("<case>", Rmw::Cyclonedds)` rows (gated on
    Phase 169.5 unlock).
  - Add new `test_zephyr_cmake_case_rmw_variant_exists` rows for
    cpp × {zenoh, xrce, cyclonedds} (once cpp builds clean) and for
    c × cyclonedds (post Gap 2.B).
- `packages/testing/nros-tests/src/zephyr.rs` (runtime E2E)
  - Add cyclonedds path resolvers (example_path + build_dir) that
    mirror the existing zenoh / xrce shapes but pass
    `CONF_FILE="prj.conf;prj-cyclonedds.conf"`.
- `just/zephyr.just :: build-fixtures`
  - Add cyclonedds collapsed entries for cpp (all 6 cases) + c
    (all 6 cases, post Gap 2.B) + rust (all 7 cases, post
    Phase 169.5). Pattern matches the existing zenoh / xrce rows:
    `"native_sim/native/64|build-<lang>-<case>-cyclonedds|zephyr/<lang>/<case>|0||prj.conf;prj-cyclonedds.conf"`.

The existing `Rmw` enum in
`packages/testing/nros-tests/src/fixtures/binaries/mod.rs` may need
a `Cyclonedds` variant if it doesn't already cover the case via
the `Dds` removal — verify after Phase 169 cleanup.

---

## Acceptance criteria

- [ ] `examples/zephyr/cpp/<case>/` builds with
       `-DCONF_FILE="prj.conf;prj-<rmw>.conf"` for every
       `rmw ∈ {zenoh, xrce, cyclonedds}`. (Gap 1)
- [ ] `examples/zephyr/c/<case>/` builds with
       `-DCONF_FILE="prj.conf;prj-cyclonedds.conf"`. (Gap 2.B)
- [ ] `examples/zephyr/rust/<case>/` builds with
       `-DCONF_FILE="prj.conf;prj-cyclonedds.conf"` once
       Phase 169.5 lands `nros-rmw-cyclonedds-sys`. (Gap 2.C)
- [ ] `examples/zephyr/cpp/cyclonedds/talker-aemv8r/` still builds
       (existing path untouched).
- [ ] `phase_118_collapse` smokes cover cyclonedds for every cell
       that has a `prj-cyclonedds.conf`. (Gap 3)
- [ ] Runtime E2E tests in `zephyr.rs` exercise the cyclonedds
       round-trip end-to-end for at least talker + listener.
       (Gap 3)
- [ ] No regression on 168.3 Rust collapse (zenoh + xrce + sca-zenoh).
- [ ] No regression on 168.4 C collapse (zenoh + xrce).

## Files (when 168.X lands)

- `packages/core/nros-cpp/build.rs` OR
  `packages/core/nros-cpp/src/log.rs` (Gap 1, option A); or
  `zephyr/CMakeLists.txt` C++ branch (Gap 1, option B).
- `zephyr/Kconfig` — drop `NROS_CPP_API` dep on
  `NROS_RMW_CYCLONEDDS` (Gap 2.B option B-1).
- `packages/dds/nros-rmw-cyclonedds/` — expose
  `nros_rmw_cyclonedds_register` with `extern "C"` linkage from a
  C-visible header (Gap 2.B option B-1).
- `examples/zephyr/c/<case>/prj-cyclonedds.conf` — new overlays
  (Gap 2.B).
- `examples/zephyr/c/<case>/src/main.c` — add `#elif defined(
  CONFIG_NROS_RMW_CYCLONEDDS)` branch to the `nros_support_init`
  block (Gap 2.B).
- `examples/zephyr/rust/<case>/Cargo.toml` + `.cargo/config.toml`
  + `src/lib.rs` + `CMakeLists.txt` + `prj-cyclonedds.conf` — new
  cyclonedds option (Gap 2.C, post Phase 169.5).
- `just/zephyr.just` — `build-fixtures` cyclonedds entries
  (Gap 3).
- `packages/testing/nros-tests/tests/phase_118_collapse.rs` —
  cyclonedds smokes (Gap 3).
- `packages/testing/nros-tests/src/zephyr.rs` — cyclonedds
  resolvers (Gap 3).
- `packages/testing/nros-tests/src/fixtures/binaries/mod.rs` —
  `Rmw::Cyclonedds` variant if missing (Gap 3).

## Notes

- The Phase 168.3 Rust collapse + 168.4 C collapse already pass
  smokes and runtime where applicable. This work strictly adds
  axes, never modifies existing ones.
- Cyclone DDS bypasses the Cargo dep graph on the C++ side — the
  register call is C++ static-init driven and Cyclone's full
  source closure compiles via Zephyr's own CMake (no Corrosion).
  So Gap 1 + Gap 2.B together unlock the entire C / C++ cyclonedds
  surface without a new Rust-staticlib crate.
- The aemv8r reference (`examples/zephyr/cpp/cyclonedds/talker-aemv8r/`)
  predates the collapse mechanism and stays at its current path
  per the Phase 168.6 "intentionally not collapsed" carve-out
  (one-target, one-board case).
