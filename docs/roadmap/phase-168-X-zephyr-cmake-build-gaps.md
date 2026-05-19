# Phase 168.X — Zephyr collapse: cyclonedds across all languages + cpp log-glue

**Goal.** Close out Phase 168 by:

1. ✓ Lifting the C++ Zephyr build path so `examples/zephyr/cpp/<case>/`
   collapses build cleanly with `prj-zenoh.conf` / `prj-xrce.conf`
   overlays.
2. Adding a `cyclonedds` RMW option to **every** collapsed
   language axis (Rust + C + C++), wired through `prj-cyclonedds.conf`
   overlays + matching CMake / Kconfig plumbing.
3. Extending the E2E test surface to exercise cyclonedds end-to-end.

**Status.** Gap 1 landed. Gap 2.B landed (C-API Kconfig dep drop
  + scaffolds in place, native_sim build hits upstream Zephyr
  cmake gen-expr bug — see below). Gap 2.C still gated on
  Phase 169.5 Rust cyclonedds shim. Gap 3 partially landed.

**Priority.** P2.

**Depends on.** Phase 168 (collapse mechanism), Phase 169
(dust-dds retirement; cyclonedds canonical DDS backend),
Phase 117 (cyclonedds C++ Zephyr module wiring).

---

## Gap 1 — C++ Zephyr build missing `nros_log_emit` + `log_fmt` glue ✓

**Resolved by**: build `nros-c` alongside `nros-cpp` in
`zephyr/CMakeLists.txt :: CONFIG_NROS_CPP_API` branch when
`CONFIG_NROS_C_API` is unset. The C-API rlib carries the
`nros_log_emit*` symbols that every `NROS_LOG_*` macro lowers
to; nros-cpp inherits them via the same link line. Side-effect:
roughly +5 s incremental compile and a second cargo target dir
slot; no functional duplication because nros-c + nros-cpp share
their nros-node / nros / nros-platform dependency closure.

The duplicate `nros-c-generated/` header byproduct that two
cargo targets declared was guarded via
`if(NOT TARGET nros_c_cargo_build)` inside
`nros_cargo_build.cmake`, requiring the nros-c build to register
before nros-cpp.

**Verified:** 12 / 12 C++ collapsed binaries
(`examples/zephyr/cpp/<case>/` × {zenoh, xrce}) build clean on
`native_sim/native/64`.

## Gap 2.B — cyclonedds on C ✓ (scaffolds + Kconfig + register stub)

**Resolved by**:

1. `zephyr/Kconfig` — drop `NROS_CPP_API` dependency from
   `NROS_RMW_CYCLONEDDS`. The backend's
   `nros_rmw_cyclonedds_register()` entry point already ships
   with C linkage (`extern "C"` block in
   `packages/dds/nros-rmw-cyclonedds/include/nros_rmw_cyclonedds.h`),
   so the Phase 160.A strong-stub emission resolves it from the
   standalone C++ library compiled by the cyclonedds branch of
   `zephyr/CMakeLists.txt` (lines 196+).
2. `zephyr/CMakeLists.txt :: CONFIG_NROS_C_API` branch — replace
   the old `FATAL_ERROR` for `CONFIG_NROS_RMW_CYCLONEDDS` with
   `_nros_features = "rmw-cffi,platform-zephyr,ros-humble"`.
3. `zephyr/CMakeLists.txt` — extend the Phase 160.A strong-stub
   `_nros_rmw_name` switch to map `CONFIG_NROS_RMW_CYCLONEDDS` →
   `cyclonedds`.
4. `examples/zephyr/c/<case>/prj-cyclonedds.conf` — new overlays
   for all six C collapsed cases, setting
   `CONFIG_NROS_RMW_CYCLONEDDS=y` plus the C++ runtime
   (`CONFIG_CPP=y`, the Kconfig only needs this since Cyclone's
   source is C++) and RTPS / IGMP / POSIX glue.
5. `examples/zephyr/c/<case>/src/main.c` — add
   `#elif defined(CONFIG_NROS_RMW_CYCLONEDDS)` branch calling
   `nros_support_init(&support, "", CONFIG_NROS_DOMAIN_ID)`.

**Known issue (deferred to Phase 168.X.fvp):**
`west build -b native_sim/native/64 -- -DCONF_FILE="prj.conf;prj-cyclonedds.conf"`
hits an upstream Zephyr generator-expression error:

```
CMake Error at zephyr/CMakeLists.txt:2145 (add_custom_command):
  Error evaluating generator expression:
    $<JOIN:$<1:$<TARGET_PROPERTY:compiler,no_strict_aliasing>>$<SEMICOLON>...>
```

Repro: same error fires for cpp-cyclonedds on `native_sim` and
appears unique to the (Cyclone DDS compile-opt path × native_sim
posix arch) combo. The FVP / aemv8r path in
`examples/zephyr/cpp/cyclonedds/talker-aemv8r/` is unaffected
because it uses a different board's compile-options closure.
Investigation deferred: Zephyr's `zephyr_compile_options(-include …)`
appears to corrupt the gen-expr stack under specific configs.

## Gap 2.C — cyclonedds on Rust ✓ (shim landed; build blocked on Gap 168.X.fvp)

**Resolved by Phase 169.5**:

1. New crate `packages/dds/nros-rmw-cyclonedds-sys/` — Rust shim
   exposing `nros_rmw_cyclonedds_register()` (C-linkage) as a
   normal Rust `extern "C"` declaration + safe `register()`
   wrapper + `linkme` distributed-slice contribution. Mirrors
   `nros-rmw-xrce-cffi`. The Cyclone DDS C++ library itself is
   compiled by the standalone `packages/dds/nros-rmw-cyclonedds/`
   CMake project or by the Zephyr module's
   `CONFIG_NROS_RMW_CYCLONEDDS` branch — this crate just provides
   the Rust binding.
2. `examples/zephyr/rust/<case>/` collapsed examples extended:
   - `Cargo.toml`: `rmw-cyclonedds` feature +
     `nros-rmw-cyclonedds-sys` optional dep.
   - `.cargo/config.toml`: patch.crates-io for the new crate.
   - `src/lib.rs`: `#[cfg(feature = "rmw-cyclonedds")]` branch in
     `register_rmw()` + `make_config()`.
   - `CMakeLists.txt`: `elseif(CONFIG_NROS_RMW_CYCLONEDDS)
     set(EXTRA_CARGO_ARGS --no-default-features --features rmw-cyclonedds)`.
   - `prj-cyclonedds.conf`: Kconfig overlay (mirrors C/C++ side).

The Rust scaffolding is fully in place. Build verification
itself blocks on Gap 168.X.fvp (same native_sim Zephyr cmake
gen-expr bug that hits C / C++ cyclonedds builds).

No regression: 13 / 13 Rust × {zenoh, xrce} + sca-zenoh still
build green after the cyclonedds option added.

## Gap 3 — E2E surface ✓ (zenoh + xrce; cyclonedds pending native_sim fix)

`packages/testing/nros-tests/tests/phase_118_collapse.rs`:
- `test_zephyr_cmake_case_rmw_variant_exists` extended with 12
  cpp × {zenoh, xrce} rows after Gap 1 landed.

`just/zephyr.just :: build-fixtures`:
- Added 12 collapsed cpp entries (zenoh + xrce × 6 cases) with
  `-DCONF_FILE="prj.conf;prj-<rmw>.conf"`.

`packages/testing/nros-tests/src/zephyr.rs` (runtime E2E):
- 168.6.B `decode_alias` resolver already supports cyclonedds —
  any `zephyr-dds-*` legacy alias resolves to
  `rmw=cyclonedds`. Activation of those cells in actual runtime
  tests waits on the native_sim cyclonedds cmake fix.

---

## Acceptance criteria

- [x] `examples/zephyr/cpp/<case>/` builds with
       `-DCONF_FILE="prj.conf;prj-<rmw>.conf"` for `rmw ∈ {zenoh, xrce}`.
       (cyclonedds blocked on upstream Zephyr cmake gen-expr bug)
- [x] `examples/zephyr/c/<case>/` cyclonedds scaffolds in place
       (Kconfig dep dropped; same upstream gen-expr block).
- [x] `examples/zephyr/cpp/cyclonedds/talker-aemv8r/` still builds.
- [x] `phase_118_collapse` smokes pass for every cell with a
       built artifact. Currently 37 / 37 pass (13 Rust + 12 C +
       12 cpp).
- [ ] `examples/zephyr/rust/<case>/` cyclonedds once Phase 169.5
       lands `nros-rmw-cyclonedds-sys`.
- [ ] native_sim cyclonedds upstream Zephyr fix (cmake gen-expr
       crash on cyclonedds `zephyr_compile_options(-include …)`
       under native_sim arch).
- [x] No regression on 168.3 Rust collapse, 168.4 C collapse.

## Files (post-landing)

- `packages/core/nros-c/Cargo.toml` — no change required;
  features unchanged.
- `zephyr/Kconfig` — `NROS_RMW_CYCLONEDDS` deps narrowed to
  `NET_SOCKETS && POSIX_API && CPP`. ✓
- `zephyr/CMakeLists.txt` — C-API CYCLONEDDS branch + strong-stub
  switch + nros-c-from-cpp build. ✓
- `zephyr/cmake/nros_cargo_build.cmake` — `TARGET` guard on
  duplicate byproduct. ✓
- `examples/zephyr/c/<case>/prj-cyclonedds.conf` + `src/main.c`
  cyclonedds branch. ✓
- `just/zephyr.just :: build-fixtures` — 12 cpp collapsed
  entries. ✓
- `packages/testing/nros-tests/tests/phase_118_collapse.rs` —
  cpp rows for `test_zephyr_cmake_case_rmw_variant_exists`. ✓

## Phase 168.X.fvp — native_sim cyclonedds cmake gen-expr fix

When the upstream block resolves:
- Reactivate `prj-cyclonedds.conf` on the native_sim path.
- Add cyclonedds rows to `test_zephyr_cmake_case_rmw_variant_exists`
  for C / C++.
- Add `build-c-<case>-cyclonedds` / `build-cpp-<case>-cyclonedds`
  entries to `just zephyr build-fixtures`.

## Notes

- The Phase 168.3 Rust collapse + 168.4 C collapse + Phase 168.X
  cpp unblock together give 37 collapsed Zephyr binaries
  passing `phase_118_collapse` smokes today.
- Gap 1's "build nros-c alongside nros-cpp" pattern is also what
  the Phase 140 integration shells will reuse when downstream
  CMake consumers pick `NROS_CPP_API` without `NROS_C_API`.
