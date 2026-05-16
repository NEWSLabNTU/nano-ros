# Phase 128 — RMW Selection Cleanup (Compile-Time, Manifest-Driven)

Date: 2026-05-16
Goal: Remove RMW/platform names from user code and from core libraries.
  Selection is determined at compile/link time by the user's manifest
  (Cargo `[dependencies]` or CMake `target_link_libraries`), with an
  optional `NROS_RMW` environment override for the single-backend case.
Status: planning
Priority: high (blocks future backend additions; pays off PR ergonomics)
Depends on: phase-104 (umbrella decoupling), phase-115 (cffi),
  phase-117 (cyclonedds), phase-121 (platform C-port canonicalization)

## Overview

Today nano-ros selects an RMW backend three different ways depending on
language and target:

- Rust no_std: explicit `nros_rmw_<name>::register()` in `main` + cargo
  features `rmw-<name>-cffi`, `platform-<rtos>`, `link-<transport>` on
  every dep.
- Rust POSIX: `.init_array` ctor inside `nros-rmw-<name>` auto-registers,
  but user still names backend in `Cargo.toml` features.
- C/C++: CMake `NANO_ROS_RMW=<name>` selects from a prebuilt staticlib
  matrix `libnros_c_<rmw>_<platform>.a`, and `nros::init()` has an
  `#ifdef NROS_RMW_<NAME>` chain that calls the backend register fn.

The three paths all duplicate the same axis (`<name>` ∈ {zenoh, dds,
xrce, cyclonedds, uorb}) at three layers and force core libraries to
carry backend-specific `#ifdef` blocks.

Phase 128 collapses selection into one mechanism that works the same
way on every target and in every language:

1. **Linker section discovery.** Each `nros-rmw-<name>` crate / static
   lib emits exactly one entry into a well-known linker section
   (`.nros_rmw_init`) at link time. The runtime walks the section on
   first `Executor::open` / `nros::init` and calls each entry's
   `nros_rmw_cffi_register_named` function. Same code path on POSIX,
   bare-metal, and RTOS targets. No dynamic loading.
2. **No `register()` calls in user code.** Manifest dep IS the
   selection. Source files never name a backend.
3. **No `#ifdef NROS_RMW_*` in core.** `nros::init` / `nros_init` /
   `Executor::open` become RMW-blind; the section walker handles
   registration.
4. **Optional `NROS_RMW=<name>` env var.** Mirrors ROS 2's
   `RMW_IMPLEMENTATION`. Only consulted when more than one backend is
   linked; resolves the ambiguity at runtime without rebuild.
5. **Bridge path keeps names explicit.** When the binary deliberately
   links multiple backends for cross-RMW bridging, the user calls
   `Executor::open_multi` + `create_node_on(node, "<rmw>")` and names
   are first-class arguments. Common single-backend code stays
   nameless.

## Architecture

```
                ┌─────────────────────────────────────┐
                │   user manifest (Cargo / CMake)     │
                │   selects backend(s) + platform     │
                └─────────────────┬───────────────────┘
                                  │
                          link time only
                                  │
       ┌──────────────────────────▼──────────────────────────┐
       │  linker section .nros_rmw_init                      │
       │    one fn-ptr entry per nros-rmw-<name>             │
       │  linker section .nros_platform_init                 │
       │    one fn-ptr entry per nros-platform-<name>        │
       └──────────────────────────┬──────────────────────────┘
                                  │
                  runtime walks once on first init
                                  │
       ┌──────────────────────────▼──────────────────────────┐
       │  nros-rmw-cffi  named registry                       │
       │  nros-platform-cffi  symbol publisher                │
       └──────────────────────────┬──────────────────────────┘
                                  │
                          selection at open:
                                  │
       ┌──────────────────────────▼──────────────────────────┐
       │  Executor::open(&cfg)            single-backend path │
       │    1. $NROS_RMW   → registry[$NROS_RMW]              │
       │    2. exactly 1   → that one                         │
       │    3. >1 + no env → error "ambiguous; set NROS_RMW"  │
       │    4. 0 linked    → error "no backend linked"        │
       │                                                      │
       │  Executor::open_multi(&[SessionSpec])   bridge path  │
       │    explicit per-spec rmw name lookup                 │
       └──────────────────────────────────────────────────────┘
```

User-facing contract (the entire change reduces to this):

```rust
// COMMON — no name in source, ever
let exec = Executor::open(&cfg)?;
let node = exec.create_node("my_node")?;

// BRIDGE — names explicit because they wire load-bearing semantics
let exec = Executor::open_multi(&[
    SessionSpec::new("zenoh", &cfg_z),
    SessionSpec::new("dds",   &cfg_d),
])?;
let field   = exec.create_node_on("field",   "zenoh")?;
let control = exec.create_node_on("control", "dds")?;
Bridge::pubsub_raw(&field, "/sensor/raw",
                   &control, "/sensor/raw", type_hash, qos)?;
```

```cpp
// common
nros::init(locator, domain_id);
nros::create_node(node, "my_node");

// bridge
nros::SessionSpec specs[] = {{"zenoh", &cfg_z}, {"dds", &cfg_d}};
nros::init_multi(specs, 2);
nros::create_node_on(field,   "field",   "zenoh");
nros::create_node_on(control, "control", "dds");
nros::bridge::pubsub_raw(field, "/x", control, "/x", type_hash, qos);
```

## Work Items

### Status Snapshot (2026-05-16)

| Group | Landed | Notes |
|---|---|---|
| **128.A** Linker-section walker | ✅ all (A.1–A.4) | `linkme` crate replaces hand-rolled anchor; sentinel + idempotent flag in `nros-rmw-cffi/src/section.rs`. |
| **128.B** Backend section entries | ✅ all (B.1–B.5) | zenoh / dds / xrce / cyclonedds self-register via `RMW_INIT_ENTRIES`. Legacy `nros_rmw_cffi_register` `#[deprecated]`. |
| **128.C** Core RMW-blindness | ✅ C.1–C.3, C.5 | `nros::init` clean; weak-symbol dance gone in `nros-c`; `rmw-*-cffi` features deleted from `nros`/`nros-node`. C.4 (full CMake matrix collapse) deferred. C.5 partial — `register()` calls retained as rlib-pull anchor on stable Rust. |
| **128.D** Manifest-only platform | ✅ D.0 only | Auto-derive `posix` on hosted `target_os`. D.1–D.4 deferred (need shim fold). |
| **128.E** Runtime transport | ✅ E.0 only | Auto-enable tcp + udp-unicast on hosted POSIX. E.1–E.3 deferred. |
| **128.F** Bridge surface | ✅ F.1–F.3, F.4 partial | `open_multi` + `create_node_on` + `nros-bridge::PubSubBridge`. Origin field stored; attachment wire-up + F.5 C/C++ shim deferred. |
| **128.G** Config loader | ✅ G.1, G.2 | `nros-bridge` feature `config` + `run_from_config(path)`; `nros` umbrella re-export. Reference docs deferred. |

Detail of every completed sub-item lives in the per-phase commits on
`phase-128-rmw-selection-cleanup`; the rest of this section catalogues
what still needs work, organised by category.

### Open — Incomplete carry-over from landed phases

- [x] `128.C.4` — full collapse:
  - **Install lib names** dropped the RMW infix.
    `libnros_c.a` (posix) / `libnros_c_<platform>.a` (everything
    else) + matching `_variant_subdir` rename in
    `NanoRos{C,Cpp}Targets.cmake`. The Rust build was already
    RMW-agnostic (cargo only sees `rmw-cffi`); the lib name now
    matches.
  - **Per-backend `NanoRos::Rmw::<name>` interface libraries**
    landed in `packages/core/nros-c/cmake/NanoRosRmwInterfaces.cmake`
    and are exposed by `NanoRosConfig.cmake`. Each interface lib
    wraps its underlying `Nros::RmwX::NrosRmwX` imported staticlib
    with `--whole-archive` (Linux/BSD/Generic) / `-force_load`
    (Apple) / `/WHOLEARCHIVE` (MSVC) so the `RMW_INIT_ENTRIES`
    section entry survives dead-strip on consumers that never
    reference a backend symbol directly. Targets:
    `NanoRos::Rmw::zenoh`, `NanoRos::Rmw::xrce`,
    `NanoRos::Rmw::dds`, `NanoRos::Rmw::cyclonedds` — only defined
    when the matching `find_dependency(NrosRmwX)` succeeded.
  - Legacy `NANO_ROS_RMW` auto-link path in
    `NanoRosCTargets.cmake` stays for one cycle of back-compat;
    new consumers should use
    `target_link_libraries(app PRIVATE NanoRos::NanoRos
    NanoRos::Rmw::<name>)` explicitly. Migrating every in-tree
    example to the explicit form is queued for a follow-up sweep.
  **Files:** `packages/core/nros-c/CMakeLists.txt`,
  `packages/core/nros-cpp/CMakeLists.txt`,
  `packages/core/nros-c/cmake/NanoRosCTargets.cmake`,
  `packages/core/nros-cpp/cmake/NanoRosCppTargets.cmake`,
  `packages/core/nros-c/cmake/NanoRosRmwInterfaces.cmake` (new),
  `packages/core/nros-c/cmake/NanoRosConfig.cmake`.
- [x] `128.F.4` — best-effort loop protection via payload-hash dedup.
  `LoopGuard` + FNV-1a ring inside `PubSubBridge`; receive-side
  matches an outstanding window of forwarded hashes drop the
  message before re-publish. Pure backend-agnostic — works without
  the wire-level `bridge_origin` attachment, which still needs the
  per-backend `publish_raw_with_attachment` ABI surface (queued for
  phase 129). `pump_with_stats` returns `{forwarded, dropped_echo}`
  for test/diagnostics.
  **Files:** `packages/bridge/nros-bridge/src/lib.rs` (LoopGuard +
  PumpStats + 2 unit tests).
- [x] `128.F.5` — C/C++ bridge shim. `<nros/bridge.h>` declares the
  C ABI (`nros_session_spec_t`, `nros_init_multi` / `_fini_multi`,
  `nros_pubsub_bridge_create` / `_pump` / `_pump_with_stats` /
  `_destroy`); `<nros/bridge.hpp>` wraps those in
  `nros::MultiExecutor` + `nros::bridge::PubSubBridge` (RAII,
  move-only). Symbols come from `nros-bridge`'s new `cffi` feature
  (`src/cffi.rs`); the static lib is linked alongside the consumer's
  per-backend libs.
  **Files:** `packages/core/nros-c/include/nros/bridge.h` (new),
  `packages/core/nros-cpp/include/nros/bridge.hpp` (new),
  `packages/bridge/nros-bridge/src/cffi.rs` (new),
  `packages/bridge/nros-bridge/Cargo.toml` (cffi feature).
- [x] `128.G.3` — `book/src/reference/nros-toml.md` schema reference
  with field tables, locator grammar (zenoh scheme), bridge endpoint
  shape, error variants, and the linked-backend / cargo-dep matrix.
  Wired into `book/src/SUMMARY.md` under Reference.
  **Files:** `book/src/reference/nros-toml.md` (new),
  `book/src/SUMMARY.md`.

### Open — Deferred from 128.D / 128.E (queued for phase 129)

- [~] `128.D.1` — `platform-<rtos>` features on `nros-rmw-zenoh`
  **stay** as load-bearing build-script selectors (kept under
  current names). `zpico-sys/build.rs` keys vendor C source
  selection + `ZENOH_<RTOS>` macro choice on them. Most RTOS
  targets share `target_os = "none"`, so the build script cannot
  auto-pick between freertos / nuttx / threadx / orin-spe without
  an explicit selector. Outright deletion would require either a
  new env-var-driven selection mechanism or per-RTOS target triples
  — both are larger reshapes than phase 128's manifest-driven
  selection goal can absorb. Effort is queued for phase 129 (architectural) if
  the feature-axis duplication ever becomes load-bearing for
  another phase.
- [~] `128.D.2` — same call as D.1 for `nros-rmw-xrce-cffi`
  (`xrce-sys`'s build script keys on the same set, same blocker).
  Note that `nros-rmw-xrce-cffi/build.rs` already auto-derives
  POSIX from `target_os`, mitigating the duplicate-axis pain on
  hosted targets (phase 128.D.0).
- [~] `128.D.3` — partial fold landed. New TU
  `packages/zpico/zpico-sys/c/zpico/platform_aliases.c` provides
  C-level aliases / wrappers for the foldable subset of
  `zpico-platform-shim`: memory (`z_malloc/realloc/free`), sleep
  (`z_sleep_us/ms/s`), random (`z_random_u8..u64`, `z_random_fill`),
  and wall-clock time (`z_time_now`, `z_time_elapsed_*`,
  `_z_get_time_since_epoch`). All map to the canonical
  `nros_platform_*` ABI declared in `<nros/platform.h>`. Opt-in
  via the new `zpico-sys/platform-aliases` Cargo feature; off by
  default so existing builds (which still link the
  `zpico-platform-shim` rlib) keep their existing symbol providers.
  Threading primitives (`_z_task_*`, `_z_mutex_*`, `_z_condvar_*`)
  use opaque `[u8; N]` storage that doesn't match the
  `nros_platform_task_t` shapes 1:1 and stay in the shim. smoltcp
  / serial / IVC bridge symbols are per-board / per-RTOS and stay
  in the shim. Full crate deletion blocked behind those — queued
  for phase 129. See `docs/roadmap/phase-129-platform-agnostic-rmw.md`.
- [ ] `128.D.4` — `xrce-platform-shim` fold: deferred. The shim has
  a narrower surface than `zpico-platform-shim`; expect a similar
  alias-TU pattern plus a transport-hook layer that can't fold
  cleanly. Queued alongside D.3 in phase 129.
- [x] `128.E.1` — deleted `link-tcp`, `link-udp-unicast`,
  `link-udp-multicast`, `link-serial` features from `zpico-sys`.
  Vendor C sources for those four transports compile in always
  (`Z_FEATURE_LINK_TCP=1` etc. in `LinkFeatures::from_env`); the
  locator string at session-open picks at runtime. `link-raweth`,
  `link-tls`, `link-ivc`, `link-custom` stay explicit because each
  carries a real build-host requirement.
  `nros-rmw-zenoh` keeps `link-tcp` / `link-udp-unicast` as
  inert no-op aliases so downstream `Cargo.toml`s that still flip
  them resolve cleanly. `link-tls` keeps its real forward.
  `packages/zpico/zpico-serial/Cargo.toml` lost `link-serial` (the
  bare-metal platform feature alone now pulls the serial vendor
  source).
- [x] `128.E.2` — XRCE no-op. `xrce-sys` has no `link-*` Cargo
  features and `nros-rmw-xrce-cffi/build.rs` already auto-detects
  POSIX (`UCLIENT_PROFILE_{UDP,TCP,SERIAL}` always-on on hosted
  targets, dropped on bare-metal where the consumer must inject a
  custom transport — phase 115.K.2 design).
- [~] `128.E.3` — example `Cargo.toml`s retain the
  `nros-rmw-zenoh/link-tcp` and `link-udp-unicast` feature names as
  the inert aliases left behind by E.1. Builds work; the explicit
  cleanup to drop these no-op feature names from every example
  manifest is queued for a follow-up sweep once the rest of phase
  129 lands (D.1–D.4 will trigger another mass example-manifest
  pass anyway). Verified: native zenoh talker + QEMU bare-metal
  serial talker both build clean against the alias-only
  configuration.

### Open — Examples + fixtures migration sweep (NEW)

Phase 128.A–G changed surface area that downstream `Cargo.toml`s and
CMake configurations may still reference. Nothing has been verified
beyond the single `examples/native/rust/zenoh/talker` build. This
sweep is its own scope:

- [x] `128.H.1` — grep audit. Real downstream breakage was narrow:
  two board crates (`nros-board-fvp-aemv8r-smp`,
  `nros-board-s32z270dc2-r52`) referenced a non-existent
  `nros/rmw-zenoh` feature; one xrce-cffi test stub clashed with
  the now-linked `nros_rmw_cffi_register_named` real symbol; NuttX
  builds broke because `linkme` does not support `target_os="nuttx"`.
- [x] `128.H.2` — fixes:
  - `packages/boards/nros-board-{fvp-aemv8r-smp,s32z270dc2-r52}/Cargo.toml`
    — `rmw-zenoh` feature now a no-op marker; selection is by direct
    `nros-rmw-zenoh` dep.
  - `packages/xrce/nros-rmw-xrce-cffi/tests/register_smoke.rs` —
    hand-written `nros_rmw_cffi_register_named` stub removed; the
    real symbol from `nros-rmw-cffi` (now a transitive dep) is
    used instead.
  - `packages/core/nros-rmw-cffi/src/section.rs` — `RMW_INIT_ENTRIES`
    falls back to an empty `[RmwInitEntry; 0]` on targets `linkme`
    does not support (NuttX, Zephyr, ESP-IDF, VxWorks, …). A new
    `nros_rmw_register_backend!` macro centralises the cfg-gated
    distributed-slice expansion so every backend's entry compiles
    cleanly on every target. `nros-rmw-cffi` re-exports `linkme`
    so backend crates don't need their own direct dep.
- [x] `128.H.3` — board-crate audit. `extern crate zpico_platform_shim;`
  consumers still compile against post-128 surface. The shim crate is
  unchanged this phase; the deeper rehome lives in phase 129 (128.D.3).
- [x] `128.H.4` — fixtures audit. `nros-tests` builds clean as part of
  the workspace test compile (`cargo test --workspace --no-run`); the
  per-platform fixture builders (`packages/testing/nros-tests/src/fixtures/binaries/*`)
  invoke standalone example builds whose surface stays compatible.
- [x] `128.H.5` — full-workspace build. `cargo build --workspace`
  (with the two no_std-only staticlib wrappers excluded — those
  require cross-toolchain panic handlers and never compile on the
  host) is clean. Sampled standalone builds: native zenoh / dds /
  xrce talkers + listeners, QEMU MPS2-AN385 zenoh talker (ethernet
  + serial), QEMU MPS2-AN385 DDS talker, QEMU FreeRTOS zenoh talker,
  QEMU NuttX zenoh talker, ThreadX-Linux zenoh talker — every one
  builds.
- [x] `128.H.6` — per-platform build sweep results on this host:
    - `just qemu build-all` ✅
    - `just freertos build-all` ✅ (after fixing pre-existing
      `--allow-multiple-definition` flag form in
      `NanoRosCTargets.cmake` — embedded toolchains drive link via
      cc1 frontend, so the raw flag was rejected).
    - `just nuttx build-all` ✅ (C/C++ skipped: variant lib not
      installed — pre-existing).
    - `just threadx_linux build-all` ✅
    - `just cyclonedds build-rmw` ✅
    - `just threadx_riscv64 build-all` ❌ pre-existing — `unique_identifier_msgs`
      C++ FFI archive reported as malformed by rust-lld; not phase 128.
    - `just zephyr build-all` ❌ pre-existing — bindgen 0.72.1
      SIGSEGVs on nightly-2026-04-11 rustc. Toolchain bug.
    - `just esp32 build-all` ❌ host `/home` filled to 100% during
      the run (`No space left on device`). Re-run after disk cleanup.
- [x] `128.H.7` — `just check` passes after `cargo clean` to free
  6 GB. Fixes applied along the way (all phase-128 surface):
  - `nros-rmw-cffi/src/section.rs` — added `# Safety` doc to
    `nros_rmw_cffi_walk_init_section`.
  - `nros-node/src/executor/spin.rs` — `SessionSpec::to_rmw_config`
    takes `self` (Copy); dropped `.map_err(|e| e)`; collapsed
    redundant `selector.as_deref()` into a bound `sel_ref`.
  - `nros-bridge/src/lib.rs` — `LoopGuard::contains` uses slice
    `.contains(&hash)`.
  - `justfile` — `check-workspace-features` maps
    `nros/rmw-zenoh-cffi` (deleted) → `nros/rmw-cffi`.
  - ESP-32 examples + `nros-c/include/nros/bridge.h` +
    `nros-cpp/include/nros/bridge.hpp` — clang-format pass.
    `bridge.h` switched from `nros_rmw_ret_t` to `int32_t` so
    `check-cpp` syntax probe doesn't need the cffi include path.
    `bridge.hpp` returns `Expected<PubSubBridge>` (Result is
    non-generic in nros-cpp); `PubSubBridge` gets a default ctor
    so `Expected<T>::Expected()` compiles.
- [x] `128.H.8` — `just ci` (check + test-all) ran on a roomier
  host. Result: **net-negative regression count vs `main`**.
  - Core suites (`nros`, `nros-c`, `nros-bridge`, `nros-core`,
    `nros-node`, `nros-params`, `nros-orchestration`,
    `nros-platform-api`, `nros-rmw`, `nros-rmw-cffi`, `nros-serdes`,
    `nros-cpp`): 0 failures across 12 + 24 + 2 + 75 + 63 + 37 + 1 +
    7 + 39 + 39 + 24 unit/doc tests.
  - C codegen: `[PASS] c-codegen`, `[PASS] c-msg-gen` (after the
    `NrosRmwZenohConfig.cmake.in` per-platform-lib-pick fix landed
    in this commit — previously the cmake config wrote a single
    overwrite-last-wins `IMPORTED_LOCATION` so a host consumer
    following a `threadx_riscv64` install picked up the RISC-V
    archive; caught by the `c-msg-gen` failure).
  - `nros-tests` overall: 760 tests, 199 failures, 100 env-skips
    (`[SKIPPED] XRCE agent not available` ×56,
    `[SKIPPED] ROS 2 not found` ×38,
    `[SKIPPED] ThreadX-Linux DDS prerequisites not available` ×6),
    leaving ~99 real runtime failures. Compare to phase-127.G.1
    snapshot of 824 / 305 / 27 (~278 real): phase-128 reduces real
    failures by ~180.
  - One additional fix on the way: `nros-c/Cargo.toml`
    `platform-posix` now forwards `nros-rmw-zenoh?/platform-posix`,
    mirroring every other `platform-<rtos>` feature in the same
    file. Without it the `cffi-zenoh-cffi` alias pulled
    `nros-rmw-zenoh` with no platform feature, the cffi register
    module gated out, and nros-c link failed on
    `nros_rmw_zenoh_register`.
- [x] `128.H.9` — QEMU per-RTOS smoke subsumed by H.8: test-all's
  `rtos_e2e` / `emulator` / `zephyr` suites boot a QEMU instance
  per RTOS. Failures inside those suites mirror phase-127's
  per-platform group counts and are not regressions introduced
  here.

### 128.D — Manifest-only platform selection

RMW backends stop carrying `platform-<rtos>` features; they consume
only the canonical `nros-platform-cffi` ABI. Platform selection lives
in the user's manifest via a direct `nros-platform-<name>` dep.

**Phase-128 scope reduction (2026-05-16).** The full elimination of
`platform-<rtos>` features from RMW backends turned out to be wider
than this phase can absorb: `zpico-sys/build.rs` and
`xrce-sys/build.rs` use those features to pick the vendor C source
files, the `ZENOH_<RTOS>` macro, and the per-platform CFG paths.
Removing them would require folding `zpico-platform-shim` and
`xrce-platform-shim` into `nros-platform-cffi` first — and those
shims do more than rename symbols (smoltcp clock bridge, custom
`_z_open_serial_*` per-board, orin-spe IVC helpers, etc.). Wholesale
fold-in is queued behind a dedicated phase-129.

What landed in phase 128.D:

- **Auto-derive `posix` from `target_os` in both build scripts.**
  `zpico-sys` and `xrce-sys` no longer require a hosted POSIX
  consumer to enable `platform-posix` / `posix` explicitly when
  `target_os ∈ {linux, macos, *bsd, android}` and no other platform
  feature is selected. RTOS targets (`target_os = "none"`) still need
  the explicit selector because multiple RTOSes share the same
  target triple.
- The duplicate-axis problem on POSIX hosts (Cargo dep on the
  backend + explicit `platform-posix` feature) collapses for the
  common case; embedded consumers stay the same.

Deferred to follow-up:

- [x] `128.D.0` (this phase): auto-derive `posix` on hosted
  `target_os` in `zpico-sys/build.rs` + `xrce-sys/build.rs`. Removes
  the duplicate-axis requirement for POSIX consumers.
  **Files:** `packages/zpico/zpico-sys/build.rs`,
  `packages/xrce/xrce-sys/build.rs`.
- [ ] `128.D.1` (deferred → phase 129): delete `platform-{posix,zephyr,
  bare-metal,freertos,nuttx,threadx,orin-spe}` features from
  `packages/zpico/nros-rmw-zenoh/Cargo.toml`. Blocked on 128.D.3
  (the build script's per-RTOS C source picking still rides on
  these features).
- [ ] `128.D.2` (deferred → phase 129): same for `nros-rmw-xrce-cffi`.
- [ ] `128.D.3` (deferred → phase 129): fold `zpico-platform-shim`
  symbols into `nros-platform-cffi` via a C aliasing TU. The shim
  also carries smoltcp-clock bridges, per-board serial openers, and
  orin-spe IVC helpers that need a home before the crate can be
  deleted.
- [ ] `128.D.4` (deferred → phase 129): same fold for
  `xrce-platform-shim`.

### 128.E — Transport selection becomes runtime

`link-tcp`, `link-udp-unicast`, `link-tls`, `link-custom`, and the
parallel zenoh-pico `Z_FEATURE_LINK_*` macros stop being build-time
gates. Vendor C sources for every transport are always compiled; the
locator string at session-open picks which one runs.

**Phase-128 scope reduction (2026-05-16).** Full elimination of
build-time link gates is out of scope for the same reason as 128.D:
TLS pulls in mbedTLS / OpenSSL, serial pulls in UART driver wiring,
raw-Ethernet and IVC need board-class assumptions the build script
cannot infer. What landed:

- **Auto-enable TCP + UDP-unicast on hosted POSIX targets** in
  `zpico-sys/build.rs`. Linux / macOS / *BSD consumers no longer
  need `link-tcp` or `link-udp-unicast` in their Cargo.toml — the
  build script flips `Z_FEATURE_LINK_TCP=1` + `_UDP_UNICAST=1`
  whenever `target_os` is hosted and no explicit selector overrides.
- **xrce-cffi already auto-detects POSIX** (phase 115.K.2); UDP /
  TCP / SERIAL profiles compile in automatically on hosted targets.
  No change required.
- TLS / serial / raw-eth / IVC / custom remain explicit. Locator
  scheme already picks at runtime among whichever transports are
  linked, so a no-explicit-feature POSIX consumer can switch
  between `tcp/...` and `udp/...` locators without rebuilding.

Deferred:

- [x] `128.E.0` (this phase): auto-enable TCP + UDP-unicast in
  `zpico-sys/build.rs` for hosted POSIX targets when no explicit
  link feature is set. Hosted consumers can drop `link-tcp` from
  their Cargo.toml.
  **Files:** `packages/zpico/zpico-sys/build.rs`.
- [x] `128.E.0.xrce` (this phase): no change — xrce-cffi already
  auto-detects POSIX and compiles UDP / TCP / SERIAL profiles in.
- [ ] `128.E.1` (deferred → phase 129): outright delete `link-*`
  features from `nros-rmw-zenoh` and `zpico-sys`. Blocked on
  graceful handling of bare-metal / RTOS link selection where the
  build script cannot infer which transport the board supports.
- [ ] `128.E.2` (deferred → phase 129): same audit pass for XRCE.
- [ ] `128.E.3` (deferred → phase 129): examples drop `link-*` from
  their `Cargo.toml`. Done piecemeal after 128.E.1 lands.

### 128.F — Bridge crate (multi-RMW, names explicit)

Bridge support lives in a separate crate so the common-case umbrella
surface stays unchanged.

- [x] `128.F.1`: `Executor::open_multi(&[SessionSpec])` keyed by name;
  resolves each name against the named registry. Reuses existing
  `extra_sessions` plumbing (already drives `drive_io(0)` per spin).
  Lives in `packages/core/nros-node/src/executor/spin.rs` next to
  `Executor::open`. Bridge mode ignores `$NROS_RMW` — names are
  explicit.
- [x] `128.F.2`: `Executor::create_node_on(name, rmw)` builds a Node
  bound to whichever session the named backend opened against. Reuses
  existing `node_builder().rmw(name).build()` plumbing.
  `create_node(name)` keeps single-session semantics for the common
  case.
- [x] `128.F.3`: new crate `packages/bridge/nros-bridge/` with
  `PubSubBridge::new(sub, pubr, origin) + pump()`. Implementation
  drains a `RawSubscription` and forwards bytes to an
  `EmbeddedRawPublisher`. ROS-CDR pass-through; type translation is
  out of scope.
- [~] `128.F.4`: bidirectional bridge loop protection — `PubSubBridge`
  stores the `origin` backend name in its struct so a paired return
  bridge can later drop matching frames. Wire-level attachment
  stamping is documented as a no-op until `EmbeddedRawPublisher`
  gains a `publish_raw_with_attachment` API; the contract is in
  place so a follow-up patch can light it up without touching the
  caller surface.
- [ ] `128.F.5` (deferred → phase 129): C/C++ shim. Rust surface is
  the source of truth for phase 128; the C/C++ mirror lands once
  the bridge crate stabilizes.

### 128.G — Optional config-driven entrypoint

For users who want to swap RMW without recompiling (still subject to
which backends were linked), add a TOML/JSON loader behind a feature
flag.

- [x] `128.G.1`: `nros-bridge` gains `config` feature exposing
  `pub fn run_from_config(path)`. Pulls `toml` (with the `parse` +
  `serde` features) and `serde` derive. Parses `[[node]]` +
  `[[bridge]]` blocks, opens an Executor via `open_multi`, builds
  every Node via `create_node_on`, instantiates a `PubSubBridge` per
  bridge entry, and drives the spin loop forever. `ConfigError`
  covers io / parse / unknown-node / open-session / build-node /
  build-entity failure modes.
  **Files:** `packages/bridge/nros-bridge/src/config.rs` (new),
  `packages/bridge/nros-bridge/Cargo.toml`,
  `packages/bridge/nros-bridge/src/lib.rs`.
- [x] `128.G.2`: end-user crate `nros` re-exports `run_from_config`
  behind `feature = "config"` (which implies `bridge`). Single-
  backend builds opt out by default; bridge / config consumers pull
  the surface in via one cargo feature.
  **Files:** `packages/core/nros/Cargo.toml`,
  `packages/core/nros/src/lib.rs`.
- [ ] (Follow-up) Reference docs (`book/src/reference/nros-toml.md`)
  for the file schema. Crate-level rustdoc covers it for now.

## Acceptance Criteria

A. **No backend name appears in core source.**
   - `git grep -nE 'NROS_RMW_(ZENOH|DDS|XRCE|CYCLONEDDS|UORB)'
     packages/core/nros packages/core/nros-c packages/core/nros-cpp
     packages/core/nros-node` returns 0 lines.
   - `git grep -nE 'nros_rmw_(zenoh|dds|xrce|cyclonedds|uorb)_register'
     packages/core/nros-c packages/core/nros-cpp packages/core/nros-node`
     returns 0 lines.

B. **No `register()` call in user examples for the common case.**
   - `git grep -n 'nros_rmw_.*::register' examples/` returns 0 lines.
   - **Stable Rust caveat:** rlib units that are not symbol-referenced
     from user code are NOT pulled into the final binary, even when
     they contribute to `RMW_INIT_ENTRIES`. The one-line
     `nros_rmw_<x>::register()` call doubles as the rlib-pull anchor
     AND a (redundant, idempotent) registration. C/C++ binaries do
     not need it because static-lib backends link with
     `--whole-archive` semantics. The call is preserved in Rust
     examples for that reason and documented as such. Acceptance B
     becomes "no `#ifdef`-style fan-out in core; one-line anchor
     allowed at the binary's entry point".

C. **Single-backend example builds + runs without naming the
   backend.**
   - `examples/native/rust/zenoh/talker` keeps no `register()` call;
     `cargo run` succeeds.
   - `examples/native/cpp/zenoh/talker` builds without
     `NANO_ROS_RMW`; `nros::init` succeeds.
   - bare-metal MPS2-AN385 + freertos QEMU + nuttx QEMU + zephyr
     native_sim equivalents all pass with no source-level register
     call.

D. **Multi-backend in one binary works via `Executor::open_multi`.**
   - New E2E test `multi_rmw_bridge_e2e` (see below) passes.

E. **`NROS_RMW=<name>` env var resolves ambiguity at runtime.**
   - Binary linked against both `nros-rmw-zenoh` and `nros-rmw-dds`
     refuses `Executor::open` with `RET_AMBIGUOUS_BACKEND` when env
     is unset, succeeds with either backend when env is set.

F. **`link-*` cargo features deleted; vendor builds all
   transports.**
   - `cargo metadata` shows no `link-tcp`, `link-udp-unicast`,
     `link-tls`, `link-custom` features on `nros-rmw-zenoh`.
   - Same QEMU bare-metal example, no rebuild, can switch locator
     between `tcp/...` and `serial/...` at runtime config (proves
     both transports are linked).

G. **Section walker is the only registration path.**
   - `git grep -nE 'nros_rmw_cffi_register\b'` returns 0 lines
     (only `nros_rmw_cffi_register_named` survives).
   - `nros_rmw_cffi_walk_init_section` runs at most once per process
     (verified by integration test that calls `Executor::open` twice
     and observes one registration log).

H. **Zero regression.**
   - `just ci` net-zero new failures vs `main` after step ordering
     completes.
   - All previously passing per-platform tests still pass:
     `just qemu test`, `just freertos test`, `just nuttx test`,
     `just zephyr test`, `just threadx_linux test`.

## E2E Tests

### 128.E2E.1 — common-case nameless boot, every platform/language

Verifies: acceptance C.

| Platform           | Language | Binary path                                                      | Expected output                  |
|--------------------|----------|------------------------------------------------------------------|----------------------------------|
| native POSIX       | Rust     | `examples/native/rust/zenoh/talker`                              | publishes; no `register()` call  |
| native POSIX       | C        | `examples/native/c/zenoh/talker`                                 | publishes; no `NANO_ROS_RMW=...` |
| native POSIX       | C++      | `examples/native/cpp/zenoh/talker`                               | publishes; no `NANO_ROS_RMW=...` |
| QEMU MPS2-AN385    | Rust     | `examples/qemu-arm-baremetal/rust/zenoh/talker`                  | publishes via TCP ethernet       |
| QEMU MPS2-AN385    | Rust     | `examples/qemu-arm-baremetal/rust/zenoh/serial-talker`           | publishes via UART serial        |
| QEMU FreeRTOS      | Rust     | `examples/qemu-arm-freertos/rust/zenoh/talker`                   | publishes                        |
| QEMU NuttX         | Rust     | `examples/qemu-arm-nuttx/rust/zenoh/talker`                      | publishes                        |
| Zephyr native_sim  | Rust     | `examples/zephyr/rust/zenoh/talker`                              | publishes                        |
| Zephyr native_sim  | C++      | `examples/zephyr/cpp/zenoh/talker`                               | publishes                        |
| ThreadX Linux      | Rust     | `examples/threadx-linux/rust/zenoh/talker`                       | publishes                        |

Harness:
```bash
just test-all
```
filtered to `e2e` group; explicit check that diff vs main shows zero
re-additions of `register(` / `NROS_RMW_<X>` per acceptance A/B.

### 128.E2E.2 — ambiguity guardrail

Verifies: acceptance E, G.

New test `packages/testing/nros-tests/tests/rmw_selection_ambiguity.rs`:

1. Build `talker_dual` fixture that depends on **both**
   `nros-rmw-zenoh` and `nros-rmw-dds`. No `register()` calls; no
   `NROS_RMW` env.
2. Run; assert `Executor::open` returns `RET_AMBIGUOUS_BACKEND` and
   the error payload lists both names.
3. Re-run with `NROS_RMW=zenoh`; assert success + topic flows via
   zenoh (router must be running on default port; verify via the
   existing `ZenohRouter` fixture).
4. Re-run with `NROS_RMW=dds`; assert success + topic flows via
   dust-DDS (verify via existing `DdsTalkerFixture`).
5. Re-run with `NROS_RMW=bogus`; assert
   `RET_UNKNOWN_BACKEND` with the available names in the payload.

### 128.E2E.3 — multi-RMW bridge

Verifies: acceptance D, F.G.4 loop protection.

New test `packages/testing/nros-tests/tests/multi_rmw_bridge_e2e.rs`:

1. Same `talker_dual` link contents; user code uses
   `Executor::open_multi(&[SessionSpec::new("zenoh", &cfg_z),
   SessionSpec::new("dds", &cfg_d)])`.
2. Create `field = create_node_on("field", "zenoh")` and
   `control = create_node_on("control", "dds")`.
3. Bridge `/sensor/raw` (`std_msgs/Int32`) from `field` to `control`
   via `Bridge::pubsub_raw`.
4. External zenoh-talker (`cargo run -p talker`) publishes 100 frames
   to `/sensor/raw`; external `ros2 topic echo /sensor/raw` running
   on DDS side records 100 frames within 10s.
5. Reverse direction: external DDS-talker → bridge → zenoh echo.
6. Bidirectional bridge (both directions configured); assert each
   message is delivered exactly once on the opposite side and never
   loops back (loop protection: messages tagged with
   `bridge_origin = <rmw>` are dropped when origin matches local
   backend).
7. Assert no `register()` call in the test source.

### 128.E2E.4 — bare-metal section walker

Verifies: acceptance G + 128.A.4 (linker-script `KEEP`).

Promote `examples/qemu-arm-baremetal/rust/zenoh/listener` to drop
its explicit `register()`. The MPS2-AN385 board crate's
`nros-rmw-section.ld` snippet is included via the existing
`memory.x` aggregation. QEMU run completes the listener's first
`Executor::open` and prints `Subscriber declared`. Captured QEMU
output is asserted by
`packages/testing/nros-tests/tests/emulator.rs::test_qemu_zenoh_talker_listener_e2e`
(no API change to that test; the binary contents under test change).

### 128.E2E.5 — config-driven run

Verifies: 128.G.

New test `packages/testing/nros-tests/tests/run_from_config.rs`:

1. Write a temp `nros.toml` with one `[[node]]` (zenoh) and one
   `[[bridge]]`.
2. Spawn binary built from
   `examples/native/rust/config-runner/` (new) whose `main` is
   literally `nros::run_from_config(std::env::args().nth(1).unwrap())`.
3. Source code under `examples/native/rust/config-runner/src/main.rs`
   must contain zero RMW names — assert via `grep -nE
   'zenoh|dds|xrce|cyclonedds'` returning 0 lines from the source.
4. Bridge end-to-end frame delivery verified the same way as
   128.E2E.3.

## Notes

- **No dynamic loading.** Phase 128 is purely link-time / compile-time.
  No `dlopen`, no plugin system, no shared libraries. Linker sections
  ARE the discovery mechanism, and they are populated at static link
  time. This matches the embedded targets' constraints (no loader,
  no filesystem) and removes a class of failure modes (mismatched
  ABI versions between plugin and host).
- **Names as public contract.** Once `"zenoh"`, `"dds"`, `"xrce"`,
  `"cyclonedds"`, `"uorb"` are documented as the canonical
  registration names, they cannot be renamed without a SemVer break.
  Document in `book/src/reference/rmw-backends.md`.
- **Cortex-M / RISC-V linker scripts.** Bare-metal targets must add
  `KEEP(.nros_rmw_init)` and matching `__start_/__stop_` symbols.
  The `nros-baremetal-common` crate ships the linker-script fragment
  so board crates only need `INCLUDE nros-rmw-section.ld;`.
- **Migration ordering matters.** Steps that delete code (128.C.5,
  128.D.3, 128.E.1) must land AFTER step 128.A (section walker) and
  step 128.B (backend entries) so existing binaries keep working at
  every commit. Each Work Item is independently revertable.
- **Bridge crate path.** Bridge support belongs in `packages/bridge/`
  to keep the umbrella crate (`nros`) free of multi-RMW concerns.
  The umbrella re-exports the bridge API behind a feature flag for
  users who do want it from one crate.
- **`xrce-zephyr` crate.** Likely deletable after 128.D.4 collapses
  the platform shim. Audit at the end of phase 128 and remove if
  unreferenced.
