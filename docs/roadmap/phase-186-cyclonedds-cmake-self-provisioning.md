# Phase 186 — CycloneDDS self-provisioning in CMake (drop the shell path), sccache-accelerated

**Goal.** Make the CycloneDDS dependency provision itself through the **build
system** (CMake / cargo+corrosion), so a bare `cmake -S <example> -B …` or
`cargo build` produces a working Cyclone-backed binary **with no `just` / shell
prerequisite**. Honor a user-supplied Cyclone (their install *or* their repo).
Drop the `just`/shell provisioning path entirely. Absorb the resulting
per-example Cyclone (re)build cost with **sccache**.

**Status.** Largely complete (2026-05-28). Keystone (`nros_provide_cyclonedds()`
+ root default), 186.2 sccache, 186.3 host-idlc, and the per-platform
self-provision for **freertos + threadx-rv64 + native** are done and validated
(bare cmake builds; e2e tests PASS); the embedded cross-probe shell path is
deleted; 186.5 docs updated. **Remaining:** full removal of the host `build.sh` /
`just cyclonedds setup` (still load-bearing for backend-CI, Zephyr idlc,
threadx-linux, and a test gate — see 186.4) — a scoped follow-up needing each
consumer migrated + a cyclone-suite revalidation.

**Priority.** P2 — the product works today via the Phase 185 `just`/shell path;
this is an architecture move so the build is self-contained and composes with
user build systems, not a bug fix.

**Depends on.** Phase 185 (the `just`/shell provisioning this **supersedes** —
185.1 build-fixtures provisioning + 185.4 shared `cross-build-ddsc.sh` are
removed here), Phase 175 (native Cyclone CMake/Corrosion path), Phase 165.perf
(sccache `CMAKE_*_COMPILER_LAUNCHER` pattern, already used for Zephyr).

---

## Overview

Today the Cyclone dependency is **built by `just`/shell** and only **consumed by
CMake** (`find_package(CycloneDDS)`):

- Host: `scripts/cyclonedds/build.sh` ← `just cyclonedds setup`.
- Embedded cross: `scripts/cyclonedds/cross-build-ddsc.sh` ← `just cyclonedds
  <rtos>-cross-probe` ← `just <rtos> build-fixtures` (Phase 185.1).
- Consume: `packages/dds/nros-rmw-cyclonedds/CMakeLists.txt:37`
  `find_package(CycloneDDS REQUIRED CONFIG)`.
- Exception: **Zephyr** already self-provisions (Cyclone as an in-tree
  west/CMake module).

Consequence: a plain `cmake`/`cargo` build of a FreeRTOS/ThreadX example fails
(`find_package` can't find Cyclone) unless `just` ran first. That doesn't compose
with a user's own build system, and a user can't point us at their own Cyclone.

**This phase moves provisioning into CMake for *every* platform** (matching what
Zephyr already does), and removes the shell/`just` provisioning. Cyclone's source
CMake exports `CycloneDDS::ddsc` as a target ALIAS (`src/core/CMakeLists.txt:120`),
so `add_subdirectory(<cyclone-source>)` yields the *same* `CycloneDDS::ddsc`
target `find_package` does — at configure time. `idlc` is already an injectable
host tool (`IDLC_EXECUTABLE`), decoupled from `ddsc`.

The cost — `add_subdirectory` rebuilds Cyclone in each example's build tree — is
absorbed by **sccache** as a `CMAKE_*_COMPILER_LAUNCHER`, the exact technique
Phase 165.perf already uses for the Zephyr cross-compiles (its comment: "Cyclone
objects … cache hits instead of full per-example recompiles"). First example
builds Cyclone; the rest hit cache.

## Architecture

### `nros_provide_cyclonedds()` — resolution order (every step user-overridable)
A CMake module in the backend replaces the bare `find_package`. Tried in order:

1. **`CycloneDDS::ddsc` already a target** → use it (a parent project already
   provided Cyclone).
2. **`find_package(CycloneDDS CONFIG QUIET)`** → a user install via
   `-DCMAKE_PREFIX_PATH` / `-DCycloneDDS_DIR`, *or* any pre-built install. Zero
   build. User install wins.
3. **Self-provision** → `add_subdirectory("${CYCLONEDDS_SOURCE_DIR}")` where
   `CYCLONEDDS_SOURCE_DIR` defaults to the pinned `third-party/dds/cyclonedds`
   submodule **but the user can point it at their own Cyclone repo**. Per-platform
   `WITH_FREERTOS`/`WITH_LWIP`/`WITH_THREADX` + cross C-flags are set as cache
   vars *before* the add_subdirectory (migrated from the shell scripts).

Then `target_link_libraries(nros_rmw_cyclonedds PUBLIC CycloneDDS::ddsc)` — works
regardless of which branch supplied the target.

### User knobs (the "customized build / own repo" requirement)
- `-DCMAKE_PREFIX_PATH=<install>` / `-DCycloneDDS_DIR=` → their prebuilt install.
- `-DCYCLONEDDS_SOURCE_DIR=<their repo>` → build their Cyclone source.
- `-DIDLC_EXECUTABLE=<their idlc>` → their host idlc.
- nothing → pinned submodule, self-provisioned.

### sccache
Pass `-DCMAKE_C_COMPILER_LAUNCHER=sccache -DCMAKE_CXX_COMPILER_LAUNCHER=sccache`
to the Cyclone sub-build (gated on `command -v sccache`, degrades to a direct
compile otherwise — the Phase 165.perf guard). sccache 0.15 is already present
(10 GiB local cache); both cross compilers (`arm-none-eabi-gcc`,
`riscv64-unknown-elf-gcc`) are cacheable. The C-flag include paths use stable
repo-root absolute paths, so objects cache across example build trees.

### What is dropped
- `scripts/cyclonedds/{build.sh, cross-build-ddsc.sh, freertos-cross-probe.sh,
  threadx-cross-probe.sh}`.
- `just cyclonedds {setup, build, freertos-cross-probe, threadx-cross-probe}`
  provisioning role; the Phase 185.1 `build-fixtures` provisioning hooks; the
  Phase 185.4 shared helper.
- The `[ -f build/cyclonedds-<rtos>-install/lib/libddsc.a ]` gates in
  `just/{freertos,threadx-riscv64}.just`.

## Work Items

### 186.1 — `nros_provide_cyclonedds()` CMake module
The resolution-order module + per-platform flag fragments (migrate `WITH_*` +
cross C-flags out of the shell scripts into CMake).

**Files**
- Create `packages/dds/nros-rmw-cyclonedds/cmake/ProvideCycloneDDS.cmake`
- `packages/dds/nros-rmw-cyclonedds/CMakeLists.txt` (replace `find_package` at :37
  with `nros_provide_cyclonedds()`)
- `cmake/platform/nano-ros-<plat>.cmake` (per-platform `WITH_*` + cross C-flags +
  board-config include dirs, sourced from the deleted shell scripts)

- [x] `CycloneDDS::ddsc` resolves via the 3-step order — validated at configure:
      find_package(prefix)→`build/install`, find_package(system)→`/opt/ros/humble`,
      self-provision(`CYCLONEDDS_SOURCE_DIR`+disable-find-package)→sccache +
      add_subdirectory. `ProvideCycloneDDS.cmake` (macro) + backend wiring.
- [x] Root CMakeLists defaults `CYCLONEDDS_SOURCE_DIR` to the pinned submodule
      (root owns third-party/) → bare cmake build self-provisions, no `-D` needed.
- [x] Per-platform Cyclone flags live in CMake — **freertos done**: the
      `nano-ros-freertos.cmake` platform module stages WITH_FREERTOS/WITH_LWIP +
      the feature trims + ddsrt FreeRTOS/lwIP include flags (gated on the
      cyclonedds RMW), and the backend adds Cyclone's internal/generated include
      roots + the ddsc whole-archive lib on the source path. **Validated:** a bare
      `cmake` of `examples/qemu-arm-freertos/rust/talker` with no
      `-DCMAKE_PREFIX_PATH` (only the cross toolchain) self-provisions Cyclone,
      compiles it for Cortex-M3, and links a 32-bit ARM ELF — no `just cyclonedds`
      pre-step. find_package path (with prefix) still selected (regression-clean).
      *(threadx + native fragments remain — same pattern.)*
- [x] `nros-rmw-cyclonedds` links `CycloneDDS::ddsc` unchanged regardless of source
      (find_package path regression-clean).

### 186.2 — sccache for the self-provisioned Cyclone sub-build
**Files**
- `packages/dds/nros-rmw-cyclonedds/cmake/ProvideCycloneDDS.cmake` (set
  `CMAKE_*_COMPILER_LAUNCHER=sccache` on the add_subdirectory scope when present)
- example/platform CMake glue that configures the sub-build

- [ ] Cyclone sub-build routes C/C++ through sccache when available, direct
      compile otherwise (Phase 165.perf guard).
- [ ] Measured: 2nd example build of the same platform is a near-total sccache
      hit for Cyclone objects (report hit-rate before/after).

### 186.3 — Decouple host `idlc`
Cross `ddsc` (`BUILD_IDLC=OFF`) gives no runnable cross idlc. Resolve a host idlc
independent of the cross `ddsc`: `find_program(idlc)` / a one-time host Cyclone
idlc build / `-DIDLC_EXECUTABLE`.

**Files**
- `packages/dds/nros-rmw-cyclonedds/cmake/{ProvideCycloneDDS,NrosRmwCycloneddsTypeSupport}.cmake`

- [ ] A self-contained `cmake` build resolves a host idlc with no `just` step
      (find_program, or a host-only sub-build of idlc).
- [ ] `-DIDLC_EXECUTABLE=<path>` override honored.

### 186.4 — Remove the shell / `just` provisioning path
**Files**
- Delete `scripts/cyclonedds/{build.sh, cross-build-ddsc.sh,
  freertos-cross-probe.sh, threadx-cross-probe.sh}`
- `just/cyclonedds.just` (drop `setup`/`build`/`*-cross-probe` provisioning; keep
  read-only `doctor`, retarget to the new state)
- `just/freertos.just`, `just/threadx-riscv64.just` (remove the 185.1 provisioning
  hooks + install gates; `build-fixtures` just builds examples — Cyclone comes
  from CMake)
- `justfile` `test-all` (revisit the 185.2 `-E` filter: the gate is no longer "is
  the install present" but "can the toolchain build Cyclone" — likely
  `command -v <cross-cc>`)

- [x] 185.2 filter re-gated on **toolchain presence** (`arm-none-eabi-gcc` /
      `riscv64-unknown-elf-gcc`) instead of the install artifact — required now
      that self-provision leaves no `build/cyclonedds-<rtos>-install`.
- [x] **freertos**: `just freertos build-fixtures` builds the Cyclone cells via
      CMake self-provision (no `-DCMAKE_PREFIX_PATH`, no cross-probe);
      `freertos-cross-probe.sh` + its `just cyclonedds freertos-cross-probe` recipe
      deleted; doctor updated. **Validated:** build-fixtures rc=0, both cells
      self-provision, `test_freertos_rust_talker_cyclonedds_boot` +
      `_local_pubsub_e2e` **PASS** on the self-provisioned binaries.
- [x] **threadx**: `nano-ros-threadx.cmake` stages the WITH_THREADX + NetX/picolibc
      flags (LTO off, board-gated to riscv64-qemu); `just threadx_riscv64
      build-fixtures` self-provisions (no cross-probe, no `-DCMAKE_PREFIX_PATH`).
      **Validated:** build rc=0 with **100% sccache hits** (flags byte-match the old
      cross-probe → cached), `test_threadx_riscv64_cyclonedds_two_qemu_pubsub`
      **PASS**. `threadx-cross-probe.sh` + the now-orphaned shared
      `cross-build-ddsc.sh` + the `just cyclonedds threadx-cross-probe` recipe
      deleted. **The embedded-Cyclone shell path is fully removed.**
- [x] **native**: `nano-ros-posix.cmake` stages the host Cyclone flags
      (`ENABLE_SECURITY/SSL/SHM=OFF` + **`BUILD_SHARED_LIBS=OFF`** — a STATIC ddsc
      linked into the app: no runtime `libddsc.so`, hence no rpath and no risk of
      ld.so resolving the app's `libddsc.so.0` against a build-flag-mismatched
      system `/opt/ros` Cyclone — which also pulls iceoryx; that was the
      transitive-iceoryx red herring). `just native build-fixtures` self-provisions
      via `-DCMAKE_DISABLE_FIND_PACKAGE_CycloneDDS=ON` (forces the pinned source) +
      `rm -rf build-cyclonedds` (clean reconfigure — `configure_if_needed` was
      reusing a stale shared-config dir). **Validated:** recipe builds rust/c/cpp
      cyclone cells static + self-contained (ldd has no libddsc/iceoryx), manual
      rust pair exchanges (`Received: 0,1`), and
      `test_native_cyclonedds_rust_talker_to_listener` (C + C++) **PASS**.
- [~] **host `build.sh` / `just cyclonedds setup`**: the **example-provisioning**
      shell path is gone (cross-probes deleted; freertos/threadx-rv64/native
      examples self-provision). `build.sh` (host `build/install`) is a *separate*
      concern and remains — it is still load-bearing for, and cannot be deleted
      until each is migrated + revalidated:
      1. **backend standalone CI** — `just cyclonedds build-rmw/test/ci` configure
         the backend with `-DCMAKE_PREFIX_PATH=build/install` + install there +
         ctest. Migrate to self-provision (`nros_provide_cyclonedds()` source) and
         re-run the ctest harness.
      2. **Zephyr-Cyclone host idlc** — `just/zephyr.just:450` hardcodes
         `build/install/bin/idlc`. Switch to `find_program`/PATH (186.3) and
         re-run the Zephyr Cyclone suite.
      3. **threadx-linux** Cyclone (host-linked, NOT migrated — only rv64 was) —
         `just/threadx-linux.just:112` gates on `build/install`. Give it a posix-
         like self-provision fragment + rewire.
      4. **test gate** — `native_api.rs:767` checks `build/install/lib/libddsc.so`;
         drop once native self-provides unconditionally.
      Each needs its own build + cyclone-suite (incl. `cyclonedds_ros2_interop`)
      revalidation; deleting `build.sh` before them breaks those paths. Tracked as
      the Phase 186 host follow-up.

### 186.5 — Docs
**Files**
- `docs/development/sdk-tiers.md` (Cyclone no longer a setup module; self-provisioned)
- `book/src/user-guide/rmw-backends.md` (update the Phase 185 note: CMake
  self-provisions; `-DCYCLONEDDS_SOURCE_DIR` / `-DCMAKE_PREFIX_PATH` knobs)
- Mark Phase 185's provisioning items superseded.

- [x] Docs describe the CMake resolution order + user knobs:
      `docs/development/sdk-tiers.md` § "CycloneDDS — self-provisioned in CMake"
      rewritten; `book/src/user-guide/rmw-backends.md` note updated
      (`-DCYCLONEDDS_SOURCE_DIR` / `-DCMAKE_PREFIX_PATH` / `-DIDLC_EXECUTABLE`
      knobs, no `just cyclonedds` pre-step). The embedded-provisioning shell path
      is gone from the docs; the host `build.sh` is documented as the remaining
      follow-up above.

### 186.6 — Remove the host `build.sh` / `just cyclonedds setup` (follow-up)
Migrate the four remaining `build/install` consumers, then delete the host
provisioning. Each step is independently landable + must be revalidated against
the relevant cyclone suite (the order below is least→most blast radius).

#### 186.6.1 — Backend standalone CI self-provisions
`just cyclonedds build-rmw/test/ci` configure the backend with
`-DCMAKE_PREFIX_PATH=build/install` + `cmake --install build/install` + ctest.
Switch to `nros_provide_cyclonedds()` (source) — drop the prefix + install step.

**Files**
- `just/cyclonedds.just` (`build-rmw` recipe ~100–111; `setup` dep; `test`/`ci`)
- `packages/dds/nros-rmw-cyclonedds/tests/` (ctest harness — confirm it links the
  self-provisioned `ddsc`)

- [ ] `just cyclonedds test` passes with no `build/install` (self-provisioned).
- [ ] `just cyclonedds ci` green.

#### 186.6.2 — Zephyr-Cyclone idlc from PATH, not `build/install`
`just/zephyr.just:~450` hardcodes `build/install/bin/idlc`. Resolve idlc
host-side (186.3 `find_program` / PATH / `-DIDLC_EXECUTABLE`) so the Zephyr
descriptor codegen doesn't need the host install.

**Files**
- `just/zephyr.just` (the `build/install/bin/idlc` reference)

- [ ] Zephyr Cyclone fixtures build with no `build/install` (idlc from PATH).
- [ ] `test(test_zephyr_dds_*_cyclonedds*)` + the Zephyr action suite PASS.

#### 186.6.3 — threadx-linux Cyclone self-provisions
threadx-linux is host-linked (not the rv64 cross). `just/threadx-linux.just:~112`
gates on `build/install`. Give the threadx-linux board/platform a self-provision
fragment (posix-like host build; ENABLE_SHM/SECURITY off, static) and rewire.

**Files**
- `just/threadx-linux.just`
- `cmake/platform/nano-ros-threadx.cmake` (a threadx-linux branch, or the board
  overlay) — host self-provision flags

- [ ] `just threadx_linux build-fixtures` Cyclone cells self-provision, no
      `build/install`.
- [ ] `test(test_threadx_linux_cyclonedds*)` PASS.

#### 186.6.4 — Drop the `native_api.rs` build/install gate
`packages/testing/nros-tests/tests/native_api.rs:~767` skips a Cyclone test on
`build/install/lib/libddsc.so` absence — obsolete once native self-provides
unconditionally.

**Files**
- `packages/testing/nros-tests/tests/native_api.rs`

- [ ] The native Cyclone test runs without the `build/install` gate.

#### 186.6.5 — Delete `build.sh` + retire `just cyclonedds setup`
Once 186.6.1–.4 land: delete `scripts/cyclonedds/build.sh`, retire the `setup` /
`build` / `clean` provisioning recipes (keep `doctor`, retarget), and drop
`build/install` from `just clean-setup`. Full cyclone-suite revalidation incl.
`cyclonedds_ros2_interop` (needs ROS 2).

**Files**
- `scripts/cyclonedds/build.sh` (delete)
- `just/cyclonedds.just` (`setup`/`build`/`clean` recipes); `justfile` (`clean-setup`)

- [ ] `git grep -nE 'build/install.*cyclone|cyclonedds/build\.sh'` is clean.
- [ ] Full `just test-all` (all tier) green — incl. `cyclonedds_ros2_interop`.

## Acceptance

- [x] **Bare build, no `just`:** `cmake -S examples/qemu-arm-freertos/rust/talker
      -B /tmp/b` (cross toolchain only, no `-DCMAKE_PREFIX_PATH`, no `just
      cyclonedds`) self-provisions Cyclone, compiles for Cortex-M3, links a 32-bit
      ARM ELF. Native equivalent links a static self-contained x86 ELF.
- [x] **User knobs:** `-DCYCLONEDDS_SOURCE_DIR=<checkout>` + disable-find-package
      builds from that source (validated); `-DCMAKE_PREFIX_PATH=<install>` /
      system resolves via find_package (validated, regression-clean).
- [x] **sccache:** threadx-rv64 build-fixtures showed **100% Cyclone cache hits**
      (1572/1574) — flags byte-match across cells/builds, cost fully absorbed.
- [x] **Parity:** freertos (`*_talker_cyclonedds_boot` + `_local_pubsub_e2e`),
      threadx-rv64 (`_two_qemu_pubsub`), native (`_rust_talker_to_listener` C+C++)
      Cyclone tests **PASS** via CMake provisioning.
- [~] The embedded **cross-probe** shell scripts + recipes are deleted (`git grep
      cross-probe` finds only comment/doc references now). The host `build.sh` /
      `just cyclonedds setup` remains by design until the 4 consumers above migrate
      (backend-CI, Zephyr idlc, threadx-linux, test gate).

## Notes

- Zephyr is the existence proof: it already self-provisions Cyclone in-tree and
  already gets sccache via Phase 165.perf. This phase brings FreeRTOS / ThreadX /
  native onto the same model and retires the shell scaffolding.
- Keep `find_package` as step 2 — a user (or a future package manager) that ships
  a prebuilt Cyclone still short-circuits the build. Self-provision is the default
  fallback, not the only path.
- Prototype on **freertos first** (one target: validate bare-cmake self-provision,
  user-source override, sccache hit-rate), then generalize to native + threadx and
  delete the shell path.
- Wire-protocol / backend logic unchanged — provisioning/build-system only.
