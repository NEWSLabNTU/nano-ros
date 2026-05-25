# Phase 181 - Fixture build SSOT (`examples/fixtures.toml`)

**Goal.** A single source of truth for every test-fixture's build options
(features, `--no-default-features`, `--target-dir`, cross `--target`, build
env, and — for C/C++ — cmake `-D` defs), consumed by BOTH the fixture build
recipes (`just <plat> build-fixtures`) AND the Phase 177.9 test-all staleness
probe. Today those options are duplicated/divergent across `just/*.just`; the
probe cannot know them, so it fell back to default features — which gives a
false staleness signal and triggers feature-thrash rebuilds. One manifest
fixes both.

**Status.** In progress (branch `phase-181-fixture-build-ssot`). 181.1–181.3
done; **181.4 done** — 7 rust platforms migrated to the SSOT + verified
(native, qemu-arm-baremetal, stm32f4, freertos, nuttx, threadx-linux,
threadx-riscv64), esp32 deferred on toolchain, zephyr/px4 N/A to the cargo
manifest. **181.5 done** — native + freertos + nuttx + threadx-linux +
threadx-riscv64 C/C++ cells migrated to the SSOT manifest + the shared
`fixtures-build.sh` cmake path (native/freertos/threadx-linux build-verified;
cyclone passes gated); 181.5.f–h (zephyr/esp32/px4) N/A to this manifest
mechanism (west-built / no C/C++ cells / no-op fixtures). Next: 181.6 (strip
recipe duplication).

**Priority.** P2 — improves `just test-all` correctness/UX (Phase 177.9
follow-up). Does not block `just ci` once landed.

**Depends on.** Phase 177.9 (the fixtures preflight + `_check-fixtures-stale`
+ `.nros-fixture.inputsig` for C/C++).

## Overview

- Manifest: `examples/fixtures.toml` — `[[fixture]]` entries, one per built
  artifact. Schema in the file header.
- Reader: `scripts/build/fixtures-manifest.py list --platform P [--lang L]
  [--rmw R]` → TSV `<dir>\t<env>\t<cargo-args>` per entry (profile added by
  the caller).
- Consumers (SSOT): the per-platform `build-fixtures` recipes (build) and
  `scripts/test/rust-fixture-stale.sh` + `just _check-fixtures-stale` (probe).
- Scope split: per-FIXTURE options live in the manifest; platform ENV
  (toolchain paths, board cache vars, SDK dirs) stays in the recipes.

## Architecture

- Rust cells: probe runs `cargo build <profile> <manifest-args>
  --message-format=json`; cargo's `fresh` flag is the staleness oracle
  (rebuilds stale = self-heal). Build recipe runs the same `cargo build`.
- C/C++ cells: `nros_cmake_fixture_build` already content-hashes inputs
  (`.nros-fixture.inputsig`) for staleness; the manifest consolidates their
  cmake `-D` BUILD options (the recipe still provides platform toolchain/board
  cache vars).

## Migration plan (ordered) + fixup checklist

Sequence chosen so the keystone (Ninja + native cmake) lands first — it
simplifies the recipes, removes the sig sprawl, and fixes the dead C/C++ probe,
making the remaining C/C++ work trivial.

1. **[x] 181.7a — Ninja flip** (keystone, 2026-05-26). Added `-G Ninja` (gated
   on `ninja` present) + a generator-mismatch `rm -rf` (reads
   `CMAKE_GENERATOR` from `CMakeCache.txt`) to both cmake helpers
   (`cmake-incremental.sh`, `fixture-matrix.sh`); callers unchanged. Verified:
   `examples/native/c/talker/build-zenoh` wiped its Make cache, reconfigured
   under Ninja (`build.ninja` present), and built OK. → fixes audit #3.
   **Finding that revises 181.7c:** `ninja -n` is NOT a clean fresh signal for
   our C/C++ cells — they link nros-c/nros-cpp via **Corrosion**, whose cargo
   build is an always-run custom command (cargo owns that incrementality), so
   `ninja -n` always lists a pending `cargo rustc` step (`[0/46] cd …/nros-c &&
   cargo rustc …`) even when nothing changed. Same class of pollution as
   `make -q`'s `cmake_check_build_system`.
2. **[x] 181.7b — native cmake pattern** (2026-05-26). Both helpers now
   "configure once (if no cache or no generated build system) + `cmake --build`"
   — no content-hash / identity sigs (cmake auto-reconfigures; per-RMW dirs have
   fixed args; the old content-hash never tracked msg/srv/action and only
   re-did what the generator already handles). `nros_cmake_fixture_build` keeps
   its `$3` param for caller compat but ignores it, and no longer writes
   `.nros-cmake-fixture.sig` / `.nros-fixture.inputsig`. Verified: native/c/talker
   (cache → skip-configure + no-op build) and a fresh native/c/listener
   (configure under Ninja + build) both OK; no new sig files. Leftover
   `.nros-cmake.sig`/`.nros-cmake-fixture.sig` in already-built dirs are now dead
   (gitignored; cleared on the next generator-mismatch wipe). → fixes audit #2.
   (`nros_fixture_cell_sig`/`nros_fixture_shared_sig` + the dead `.inputsig`
   pass in `_check-fixtures-stale` are now unused — removed in 181.7c.)
3. **[x] 181.7c — C/C++ staleness probe** (2026-05-26, `cmake --build`
   self-heal). `scripts/test/cmake-fixture-stale.sh` runs `cmake --build` on a
   cell's `build-<rmw>/` (near-no-op when fresh: cargo fingerprint + ninja/make
   skip) and reports the cell iff the output shows real compile/link work
   (`Building (C|CXX|ASM) object` / `Linking` / cargo `Compiling <crate> v`).
   Chosen over `ninja -n` because Corrosion's cargo step always shows pending
   there. `_check-fixtures-stale` now runs a cmake pass over the manifest's
   c/cpp entries (per `build-<rmw>/`, parallel), replacing the dead
   `.nros-fixture.inputsig` pass; removed the now-unused `nros_fixture_cell_sig`
   / `nros_fixture_shared_sig` + their `.inputsig` write. Verified: a fresh
   Ninja cell is silent; a first run self-healed 23 genuinely-stale (old
   Make-configured) cells and the **second run was clean (7 s)** — no
   false-stale loop, robust on both Make and Ninja dirs. → fixes audit #1 and #2
   (sig sprawl: the content-hash + identity sigs are gone; only the transient
   `.nros-cmake.sig`/`.nros-cmake-fixture.sig` files left in old build dirs,
   never re-created).
4. **181.5 — C/C++ manifest migration** (now trivial: manifest `cmake_defs` +
   unified builder + ninja-n probe), per platform 181.5.a..h.
5. **181.6 — strip duplication / converge enumeration**. Remove the redundant
   hard-coded rust builds from `native build-fixture-extras` (audit #5); point
   the broad `native build-examples` find at the manifest, or drop the
   baremetal/stm32f4 overlap (audit #4). Grep-clean = true SSOT.

## Work Items

### 181.1 — Manifest + reader (foundation)
- [x] `examples/fixtures.toml` schema + native-rust entries (roles, the
  param-services talker, tls/safety/zero-copy variants, per-RMW target-`<rmw>`
  builds, custom-transport, bench, large-buf env).
- [x] `scripts/build/fixtures-manifest.py` reader (`tomllib`/`tomli`).

### 181.2 — Rust staleness probe reads the manifest (A)
- [x] `scripts/test/rust-fixture-stale.sh` takes a manifest TSV record and
  builds with the exact features/target-dir/env (not default features).
- [x] `just _check-fixtures-stale` drives the rust pass from
  `fixtures-manifest.py list --lang rust`.
- Verified: full run over all 40 native-rust entries with their real options
  finished in ~15 s, flagged only the 2 genuinely-stale custom-transport
  cells (self-healed), no feature-thrash.
- **Files**: `scripts/test/rust-fixture-stale.sh`, `justfile`.

### 181.3 — Native rust build recipe reads the manifest (B, proof)
- [x] New `build-fixture-rust` recipe builds every native rust fixture from
  `fixtures-manifest.py list --platform native --lang rust` (codegen prep +
  manifest-driven cargo loop); `build-fixtures` now depends on it (replaced
  `build-fixture-role-examples`). Verified: builds all 40 entries with their
  exact options; `_check-fixtures-stale` clean afterwards.
- Separator fix: the reader uses `0x1F` (unit separator), not tab — tab is
  IFS-whitespace so bash `read` collapsed the empty `<env>` field and shifted
  columns. Consumers use `IFS=$'\x1f'` + a subshell `export` (not `env`, which
  mis-parses a leading `--` when the env field is empty).
- Transitional: `build-fixture-extras` still has its hard-coded rust builds
  (now cargo no-ops since options match the manifest); 181.6 removes them.
- **Files**: `just/native.just`, `scripts/build/fixtures-manifest.py`,
  `scripts/test/rust-fixture-stale.sh`.

### 181.4 — Rust fixture migration (per platform)

Shared mechanism (done): `scripts/build/fixtures-build.sh <platform> [lang]`
— DRY build loop every platform reuses. Per platform: author manifest entries
→ point the platform's rust `build-fixtures`/`build-examples` at
`fixtures-build.sh <plat> rust` → verify the build → probe stays automatic.

- [x] **181.4.a native** — 40 entries; `build-fixture-rust` → shared script.
  Verified (2 s).
- [x] **181.4.b qemu-arm-baremetal** — 12 plain-cargo entries (cross target via
  each example's `.cargo/config`); verified via the shared script (17 s).
  Recipe still built by the broad `native build-examples` find → wire in 181.6.
- [x] **181.4.c stm32f4** — plain cargo, thumbv7em; no SDK gate. Currently
  built by `native build-examples`. **Files**: `just/native.just`,
  `examples/fixtures.toml`.
- [x] **181.4.d freertos** — plain-cargo zenoh (`--no-default-features
  --features rmw-zenoh --target-dir target-zenoh`) + role examples. SDK-gated:
  `FREERTOS_DIR`/`LWIP_DIR` (direnv `FREERTOS_PORT`). Cyclone rust is cmake →
  181.5. **Files**: `just/freertos.just`.
- [x] **181.4.e nuttx** — plain cargo (`-Z build-std`, pinned nightly +
  `rust-src`). SDK-gated: nuttx kernel + external apps. **Files**:
  `just/nuttx.just`.
- [x] **181.4.f threadx-linux** — plain-cargo zenoh (`target-zenoh`) +
  `--manifest-path` bins (`logging-smoke-threadx-linux`) + `--release` host
  variants. SDK-gated: threadx/netx linux (NSOS). **Files**:
  `just/threadx-linux.just`.
- [x] **181.4.g threadx-riscv64** — plain-cargo zenoh. SDK-gated: threadx/netx
  + riscv64 toolchain. Cyclone rust is cmake → 181.5. **Files**:
  `just/threadx-riscv64.just`.
- [~] **181.4.h esp32 / qemu-esp32-baremetal** — DEFERRED (toolchain). esp32 is
  xtensa via espup (esp_idf is the `extended` SDK tier, not in the default dev
  env); qemu-esp32-baremetal is riscv32imc but built with the ESP nightly.
  Can't verify a migration without the toolchain, so left unmigrated rather
  than risk an unverified recipe change. Mechanically identical to the others:
  the recipe exports the ESP toolchain/`RUSTUP_TOOLCHAIN` (platform env) then
  calls `fixtures-build.sh esp32 rust`; author entries (esp32/rust + qemu-
  esp32-baremetal/rust talker+listener) when the toolchain is available.
  **Files**: `just/esp32.just`, `just/native.just`.
- [x] **181.4.i zephyr** — N/A to the cargo manifest by design: zephyr rust
  builds via `west build` (kernel-linked), not `cargo build`, so
  `fixtures-build.sh` cannot drive it. Zephyr fixtures (rust/c/cpp) are tracked
  under 181.5.f (west/cmake cells); they stay west-built.
- [x] **181.4.j px4** — N/A: uORB is C++-only (no rust/C fixture cells per
  CLAUDE.md); the cpp register-check is tracked under 181.5.h.

### 181.5 — C/C++ (cmake) fixture migration (per platform)

Add `cmake_defs` to manifest entries for C/C++ cells; the `build-fixtures`
recipes and `nros_cmake_fixture_build` callers read them. The manifest's
per-RMW entries map 1:1 to the separate `build-<rmw>/` dirs cmake projects use
for each RMW selection — that enumeration is the fixture-list payoff for cmake.

**Staleness for cmake cells — exploration (2026-05-25).** Unlike rust (where we
reuse `cargo build --message-format=json`'s `fresh` flag), the cmake fixtures
have **no clean detect-only "would it rebuild?"** from the build tool:
- Generator is **Unix Makefiles** (all 138 configured fixture build dirs).
  `make -q` (question mode) is **unreliable** here — cmake-generated Makefiles
  inject a `cmake_check_build_system` phantom target that always reruns, so
  `make -q` reports "stale" even on a freshly-built dir (verified: exit 1 with
  no source change). So `make -q`/`make -n` can't gate staleness.
- If the generator were **Ninja**, `ninja -C <dir> -n` (dry run) / `-d explain`
  WOULD be a clean detect-only oracle (ninja has no phantom always-run target).
  nano-ros already pins ninja ≥1.13 for the jobserver, so switching the fixture
  generator to Ninja is a future option to get cargo-style precise staleness.

  **Online convention (checked 2026-05-25).** Confirmed by CMake docs +
  Discourse + the Ninja manual: CMake has **no native "is it stale" query** —
  you delegate to the generator. The recommended convention to get a clean
  detect-only staleness check is the **Ninja generator** + `ninja -n` (dry run:
  prints what would build; `ninja: no work to do.` = up-to-date) or `ninja -d
  explain` (prints *why* each target is stale); the `-t deps`/`-t targets`
  subtools inspect the graph. The Make path is explicitly the unreliable one:
  `make -n`/`make -q` invoke CMake's `--check-build-system` (regenerates the
  build files), so they report work even when nothing changed — the CMake
  Discourse "infinite re-run loop" / "skip dependency checks" threads are about
  exactly this. **Recommendation for 181.5:** build the cmake fixtures with
  `-G Ninja` (nano-ros already pins ninja ≥1.13), then the staleness probe is
  `ninja -C <build-<rmw>> -n` per the manifest's per-RMW dirs — precise (uses
  cmake's real dependency graph, incl. linked nano-ros crates via
  add_subdirectory), detect-only (no rebuild → fits C/C++ warn-only), and the
  direct analog of cargo's `fresh`. This would supersede the coarse
  `.nros-fixture.inputsig` content hash for configured cells (keep the hash
  only as a fallback when no build dir exists yet).
- Therefore the cmake staleness signal stays a **content hash**: the existing
  `.nros-fixture.inputsig` (177.9 — sha1 of the cell sources + shared
  crates/lockfile/toolchain/SDK pins), written per `build-<rmw>/` dir on a
  successful build, compared by `_check-fixtures-stale` (warn-only — C/C++
  needs the SDK/cmake env to rebuild, so no self-heal).
- Two related sigs already exist and stay: `.nros-cmake.sig`
  (`cmake-incremental.sh`, content hash of cmake sources → gates *reconfigure*
  / re-glob) and `.nros-cmake-fixture.sig` (`fixture-matrix.sh`, identity →
  gates reconfigure). Gap: native C/C++ cells (built via
  `nros_cmake_configure_if_needed`) write `.nros-cmake.sig` but NOT
  `.nros-fixture.inputsig`, so they get no test-all staleness probe — unifying
  the native + cross cmake build paths on one builder that writes `.inputsig`
  closes that (181.5.a remaining work).

This consolidates cmake BUILD options into the manifest; staleness stays the
content hash above (no rebuild needed).

**Applying Ninja across all cmake examples/fixtures — plan.** Every fixture
cmake configure funnels through exactly two helpers, so this is a 2-line change,
not a per-example/per-recipe edit (examples' `CMakeLists.txt` stay
generator-neutral — generator is a configure-time flag):
1. **Flip the generator at the chokepoints** — add `-G Ninja` to the `cmake -S
   -B` inside `nros_cmake_configure_if_needed`
   (`scripts/build/cmake-incremental.sh`, covers native's 9 callsites) and
   `nros_cmake_fixture_build` (`scripts/build/fixture-matrix.sh`, covers
   freertos/nuttx/threadx×2). Gate on `command -v ninja` (fall back to the
   default Make generator when ninja is absent) — nano-ros already puts the
   pinned `third-party/ninja` (≥1.13) on PATH via `.envrc`.
2. **Handle the in-place generator switch** — existing `build-*/` dirs are
   Make-configured; cmake errors if the generator changes in-place. Both helpers
   read `CMAKE_GENERATOR` from `CMakeCache.txt` and `rm -rf` the build dir when
   it differs from the desired one (one-time reconfigure; also future-proof).
   (`cmake-incremental.sh` already reconfigures on its content-sig; add the
   generator-mismatch wipe; `fixture-matrix.sh` already `rm -rf`s on sig change
   — extend it to also wipe on generator mismatch since `-G` isn't in its
   caller-supplied identity sig.)
3. **Staleness probe** — `scripts/test/cmake-fixture-stale.sh <build-dir>` runs
   `ninja -C <build-dir> -n`; `ninja: no work to do.` ⇒ fresh, any planned
   command ⇒ stale. `_check-fixtures-stale` adds a cmake pass iterating the
   manifest's c/cpp entries → per `build-<rmw>/` dir → `ninja -n` (warn-only;
   C/C++ needs SDK/cmake env to rebuild). Supersedes `.nros-fixture.inputsig`
   for configured cells (keep the hash as a pre-first-build fallback).
4. **Rollout / verify** — land 1+2, wipe + reconfigure a representative cell
   (native/c/talker `build-zenoh`) and confirm it builds under Ninja, then 3;
   per-platform fixtures already route through the helpers so no recipe edits.
   Notes: zephyr (west) and esp-idf already use Ninja internally (not our
   helpers) — unaffected; the few raw `cmake -B build` calls
   (`freertos.just:253`, `threadx-linux.just:223`, `cyclonedds.just`) are
   non-fixture integration/module builds — out of scope. Ninja also fits the
   Phase 176 fifo jobserver better than recursive Make.

- [x] **181.5.a native** c/cpp — done. `fixtures-build.sh` grew a cmake branch
  (`lang c|cpp`): reads the manifest cmake records `<dir>\x1f<build-subdir>\x1f<-D
  defs>\x1f<target>`, runs `nros_cmake_configure_if_needed` + `cmake --build
  [--target …]` per cell, with the same parallel/jobserver dispatch as the rust
  branch. Platform `-D` injection (codegen tool, build-type, codegen-off, cyclone
  prefix) is recipe-supplied via the `NROS_CMAKE_EXTRA_DEFS` contract.
  `just/native.just build-fixture-extras` replaced its hand-rolled C + C++
  per-RMW loops **and** the standalone `cpp parameters` block with two
  `fixtures-build.sh native {c,cpp}` calls (parameters is a manifest row with
  `target=cpp_parameters`). Cyclone c/cpp cells stay in the gated block (need
  `just cyclonedds setup`). Staleness now covered: the 181.7c
  `cmake-fixture-stale.sh` probe runs over these manifest rows. Verified end to
  end — build (12 c + 13 cpp cells incl. parameters), idempotent no-op rerun
  (1.4 s, 0 compile/link lines), probe reports all fresh. **Files**:
  `just/native.just`, `scripts/build/fixtures-build.sh`.
  Cross-platform RMW gating: `fixtures-build.sh` grew an optional 3rd arg
  `<rmw>` (passes `--rmw` to the manifest reader) so a recipe can build one RMW
  at a time and gate optional backends (cyclone).
- [x] **181.5.b freertos** c/cpp — done. Manifest rows (12 zenoh c/cpp cells);
  recipe replaced the hand-rolled C + C++ loops with `fixtures-build.sh freertos
  {c,cpp} zenoh` + `NROS_CMAKE_EXTRA_DEFS` (toolchain, build-type, codegen
  tool). Verified: `freertos_c_talker` builds as a thumbv7m ARM ELF via the
  manifest path under bash. **Files**: `just/freertos.just`, `examples/fixtures.toml`.
- [x] **181.5.c nuttx** c/cpp — done. 12 zenoh manifest rows; recipe → two
  `fixtures-build.sh nuttx {c,cpp} zenoh` calls + EXTRA_DEFS (toolchain,
  NUTTX_DIR, NUTTX_FFI_CRATE_DIR, build-type, codegen). Same ARM-cross wiring as
  freertos. **Files**: `just/nuttx.just`, `examples/fixtures.toml`.
- [x] **181.5.d threadx-linux** c/cpp — done. 12 zenoh + 12 cyclone manifest
  rows; recipe → `fixtures-build.sh threadx-linux {c,cpp} zenoh` then a
  `libddsc.so`-gated cyclone pass. Verified: all 6 `threadx_c_*` zenoh cells
  build via the manifest under bash. **Files**: `just/threadx-linux.just`,
  `examples/fixtures.toml`.
- [x] **181.5.e threadx-riscv64** c/cpp — done. 12 zenoh + 4 cyclone
  (talker/listener) manifest rows; recipe exports the NetX/ThreadX configure
  env then runs `fixtures-build.sh threadx-riscv64 {c,cpp} zenoh` + a
  double-gated cyclone pass. The Cyclone `rust/talker` cmake/corrosion cell
  (Phase 175.B) is **not** a c/cpp manifest row, so it stays built directly via
  the retained `build_threadx_cmake_rmw` helper. **Files**:
  `just/threadx-riscv64.just`, `examples/fixtures.toml`.
- Note: the cyclone c/cpp passes (threadx-linux/-riscv64) are unverifiable here
  (no DDS host/cross libs in this tree) but reproduce the prior `-D` set exactly
  via EXTRA_DEFS; `just <plat> build-fixtures` after `just cyclonedds setup` is
  the verification.
- [x] **181.5.f zephyr** — N/A to this manifest. Zephyr fixtures build via
  `west build` / direct `ninja`, and their build options are board +
  Kconfig/`prj-<rmw>.conf`-overlay driven, not `-D` flags through our cmake
  helper. The manifest's two consumers (`fixtures-build.sh`, the staleness
  probe) never invoke west, so a manifest row would be inert. West's own
  build-system handles staleness. Left west-built (`just/zephyr.just`).
- [x] **181.5.g esp32** — N/A. `examples/esp32/` has only Rust talker/listener
  (no C/C++ cells); esp32 Rust fixtures stay deferred on the xtensa toolchain
  (181.4), and `esp32 build-fixtures` = `build-qemu` + `build-logging-smoke`
  (neither a manifest cmake cell). Nothing to migrate.
- [x] **181.5.h px4** — N/A. `px4 build-fixtures` is a no-op ("PX4 has no
  separate test fixture build today"); the cpp uORB register-check builds
  through the PX4 build system under `build-examples`, not a manifest cmake
  cell. Nothing to migrate.

### 181.7 — Simplify cmake recipes via native cmake/ninja + build audit

**Simplification (leverage cmake/ninja instead of reimplementing).** The two
custom helpers reimplement what cmake+ninja already do:
- `cmake --build` auto-reconfigures on CMakeLists/source-graph change
  (`cmake_check_build_system` is in the generated Makefile — verified), so the
  custom reconfigure content-hash (`nros_cmake_configure_if_needed`'s
  `.nros-cmake.sig`) is largely redundant.
- per-RMW build dirs have FIXED args (`build-zenoh/` is always `-DNROS_RMW=zenoh`),
  so there's no arg-change reconfigure to track (the reason both sigs existed).
- ninja gives build staleness (`ninja -n`), replacing the `.nros-fixture.inputsig`
  content hash.

  Target pattern (one shared helper, replaces both):
  ```sh
  [ -f "$bd/CMakeCache.txt" ] || cmake -S "$src" -B "$bd" -G Ninja "$@"   # configure once
  cmake --build "$bd"                                                      # auto-reconfig + incremental
  ```
  Drops `nros_cmake_configure_if_needed` + `nros_cmake_fixture_build`'s sig
  machinery and all three sig-file kinds (~36 files). Staleness probe becomes
  `ninja -C "$bd" -n`. CMakePresets were considered but cmake 3.22 lacks preset
  `include` (added in 3.23), so shared presets can't be factored without
  generating per-example presets from the manifest — heavier; revisit if cmake
  is bumped to ≥3.23.

**Build audit (2026-05-26).**
1. **C/C++ staleness probe is currently non-functional** — `.nros-fixture.inputsig`
   = 0 files on disk. The 177.9 hook writes it only via `nros_cmake_fixture_build`
   on a successful build (none since the hook landed), and native C/C++ uses
   `configure_if_needed` which never writes it. So `_check-fixtures-stale`'s C/C++
   path checks nothing today. The Ninja `ninja -n` probe fixes this properly.
2. **Three inconsistent sig mechanisms** — `.nros-cmake.sig` (31, native,
   content-hash reconfigure-gate), `.nros-cmake-fixture.sig` (5, cross, identity
   reconfigure-gate), `.nros-fixture.inputsig` (0, probe). Two helpers, same job,
   different logic → unify/drop per the simplification above.
3. **Generator = Unix Makefiles** (all 138 dirs) → `make -q` unreliable; the
   staleness gap. → Ninja.
4. **Dual fixture enumeration** — qemu-arm-baremetal + stm32f4 are built by the
   broad `native build-examples` find AND listed in the manifest → two sources;
   181.6 should point the find at the manifest (or drop the overlap).
5. **Transitional rust duplication** — `native build-fixture-extras` still
   hard-codes rust builds (now cargo no-ops); 181.6 removes them.
6. **Open build issues elsewhere** — Phase 177: 177.8 (fixtures-prebuild
   contract), 177.9 / 177.9.F (runtime E2E reruns, zephyr), 177.26 (ThreadX
   Cyclone peer interop). Phase 180 (zephyr consumable module): 20 open items —
   separate effort.

### 181.6 — Remove duplicated options from recipes (true SSOT)
- [ ] After each migration, delete the now-redundant inline build options so
  the manifest is the only source. `examples/README.md` coverage matrix and
  the manifest must agree.

## Acceptance

- [ ] `just <plat> build-fixtures` builds identical artifacts to before, with
  options sourced only from `examples/fixtures.toml`.
- [ ] `just _check-fixtures-stale` probes each rust fixture with its real
  options (no feature-thrash; fresh after a clean build).
- [ ] No fixture build options remain hard-coded in `just/*.just` (grep clean).
- [ ] `just test-all` preflight still fast when fresh.

## Notes

- Reader has no `tomllib` on Python 3.10; falls back to `tomli` (present in
  the dev env). If neither is available the manifest can't be read — keep the
  `tomli` dependency documented.
- Migration is incremental + per-platform because each `build-fixtures`
  verification is slow; land one platform at a time, keeping the manifest and
  recipe consistent so the probe never thrashes mid-rollout.
