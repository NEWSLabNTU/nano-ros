# Phase 184 — Clean-room fixture / tooling gaps

**Goal:** Close the build/test-infrastructure gaps that make a full clean-room
cycle (`just clean` → `just setup` → `just build-all` → `just build-test-fixtures`
→ `just test-all`) report failing tests even though the product code + build are
green. None of these are product regressions — they are setup/fixture-aggregation
and one codegen-link gap.

**Status:** 184.1–.5 RESOLVED 2026-05-27 (184.1/.2/.3/.5 = clean build-only;
184.4 = build-fixtures-make builds host codegen); 184.6 fixed; 184.7 RE-VERIFIED
2026-05-27 (reds were fixture-not-built preconditions + one test-timing bug, now
fixed — not live product gaps).
**Priority:** Medium.
**Depends on:** Phase 177 (archived — product fixes incl. the Cyclone idlc
re-resolve harden), Phase 175 (native Cyclone CMake/Corrosion fixtures).

## Resolution (2026-05-27) — one root cause: `clean` nuked the setup stage

All four "infra" symptoms below trace to a single bug: **`just clean` ran
`rm -rf build` (+ depended on `clean-zenohd`)**, deleting the SDK/tool installs
that `just setup` produces under `build/` (`build/{install,cyclonedds,qemu,
xrce-agent,zenohd,zephyr-cache}`). After a clean, the default (**base**-tier)
`just setup` is idempotent and doesn't rebuild them, so Cyclone (`build/install`
→ native/threadx cyclone fixtures skip their `[ -f build/install/lib/libddsc.so
]` gate), the XRCE Agent, and the patched qemu were all gone → the ~76 test-all
fails. (`build/install`, the XRCE Agent, and the patched qemu ARE installed by
`just setup tier=all` — `run xrce` + `run qemu` [→ `setup-qemu`] + `run
cyclonedds`; the clean-room run used base tier *and* clean had erased them.)

**Fix:** `clean` is now **build-stage only** — it removes examples, fixtures,
cargo `target/`, and the build-stage `build/` subdirs (`zephyr-fixtures`,
`esp32-qemu`, `qemu-zenoh-pico`), and **preserves the setup installs**. A new
**`clean-setup`** recipe removes the SDK/tool installs explicitly (full
re-setup). The per-platform `clean`s already only remove build-stage trees
(`build/cmake-*`, `build/rmw_zenoh_ws`), not the expensive installs.

**Setup vs build (the classification this enforces):**

| artifact | stage | `clean` | produced by |
|---|---|---|---|
| `build/{install,cyclonedds,qemu(patched),xrce-agent,zenohd,zephyr-cache}` | **setup** | preserved | `just setup tier=all` (per-module) |
| host `nros-codegen` CLI (`packages/codegen/packages/target/<profile>/nros-codegen`) | **setup** | preserved | `just setup` → `just workspace build-codegen` |
| `build/{zephyr-fixtures,esp32-qemu,qemu-zenoh-pico}`, `cmake-*`, `target/`, example `build-*/` | **build** | removed | `just build-all` / `build-test-fixtures` |

The host `nros-codegen` (the `nros` CLI / message-IDL codegen tool) is a
setup-stage TOOL: `just setup` builds it via `just workspace build-codegen`,
`clean` preserves it (the codegen cargo workspace is excluded), and
`clean-setup` removes it. Build recipes keep `nros_cargo_ensure_codegen_c` as an
idempotent safety net (now usually a no-op since setup built it). This is the
deeper fix behind 184.4 — `build-fixtures-make`'s FFI staging no longer depends
on a build recipe having happened to build codegen.

**Corrected workflow:** `just setup tier=all` **once** → then `clean → build-all
→ build-test-fixtures → test-all` repeatedly; the setup installs now survive
`clean`, so native/threadx Cyclone fixtures build (gate satisfied) and the XRCE
Agent + patched qemu stay present. `just clean-setup` only when you want a full
SDK re-install. **Per-platform setup-undo** (uninstall one platform's SDKs) is
deferred pending design discussion — `clean-setup` is a blanket nuke for now.

Only **184.4** (NuttX-cpp make-fixture link gap) remains — a real codegen/link
bug, unrelated to clean/setup.

## Overview

A full clean-room run on `main` (HEAD `59f47cef6`) was **build-green** —
`just clean` rc=0, `just setup` rc=0, `just build-all` rc=0 (**0 idlc-127**,
0 failures, vs exit 2 before the 177 idlc harden), `just build-test-fixtures`
rc=0 — but `just test-all` reported **760 run, 684 passed (5 flaky), 76 failed,
8 skipped**. Triaging the 76 (18 distinct test fns × params) showed **every
failure is one of four infra gaps below**, not a code regression. After applying
the per-issue workarounds, the 76 collapse to **2 residual hard fails**
(`nuttx_make_e2e` link gap + `threadx_linux_cyclonedds`, both below).

| Original test-all fail group | Count | Cause | After workaround |
|---|---|---|---|
| `c_xrce_api` | 10 | XRCE Agent not installed (184.3) | **pass** |
| `qemu_patched_binary` | 6 | patched qemu not built (184.3) | **pass** |
| `native_api` Cyclone | 9 | `build/install` nuked + native fixtures not built (184.1/184.2) | **8 pass**, 1 = threadx-linux below |
| `nuttx_make_e2e` | 1 | NuttX-cpp make-fixture link gap (184.4) | **still fails** |

## Work Items

### 184.1 — [RESOLVED]  `just clean` removes the Cyclone install; `just setup` doesn't restore it
- [x] `just clean` (→ `clean-examples clean-fixtures clean-zenohd`) removes
  `build/install/` (the Cyclone DDS install: `build/install/bin/idlc` +
  `build/install/lib/libddsc.so`). The umbrella `just setup` (default tier) is
  idempotent and, on a tree where `build/install` is gone, **does not detect or
  restore it** — its prior run was instant (rc=0) yet left `build/install`
  absent.
- Downstream: the native/ThreadX-Linux Cyclone fixture builds gate on
  `[ -f build/install/lib/libddsc.so ] && [ -x build/install/bin/idlc ]`
  (`just/native.just:169`), so with the install gone they **silently skip**
  ("native c/cpp cyclonedds skipped (run: just cyclonedds setup)"), leaving the
  fixtures unbuilt → `[SKIPPED]` test panics (counted as failures).
- **Workaround:** `just cyclonedds setup` restores `build/install` in ~4 s
  (the submodule build is cached; only the install step was missing).
- **Fix options:** (a) `just clean` should not delete `build/install` (it is an
  SDK install, not an example/fixture artifact); or (b) the umbrella `just setup`
  / `just cyclonedds setup` should re-run the install step whenever
  `build/install/{bin/idlc,lib/libddsc.so}` is missing, not skip on a stale
  "already set up" marker.

### 184.2 — [RESOLVED via 184.1]  `just build-test-fixtures` does not build the native / ThreadX-Linux Cyclone fixtures
- [x] The native Cyclone fixtures (`examples/native/<lang>/<case>/build-cyclonedds/`)
  are the Phase-175 CMake/Corrosion path built by **`just native build-fixtures`**
  (gated on 184.1's Cyclone install), and the ThreadX-Linux Cyclone fixtures by
  the threadx-linux path — **neither is invoked by the root
  `just build-test-fixtures`**. So a clean-room `build-all` + `build-test-fixtures`
  leaves them absent and the `native_api` Cyclone tests `[SKIPPED]`-fail.
- **Workaround:** `just cyclonedds setup && just native build-fixtures` →
  **8/9 native_api Cyclone pass** (the 9th is 184.5). Verified 2026-05-27.
- **Fix:** fold the native + threadx-linux Cyclone fixture builds into the
  fixture aggregation that `test-all` depends on (or document them as a required
  separate step in the clean-room runbook + the test skip messages).

### 184.3 — [RESOLVED via 184.1]  Default `just setup` omits the XRCE Agent and the patched qemu
- [x] On a fresh clean, `test-all` reports ~16 false fails because the default
  tier does not produce two tools the tests require:
  - **XRCE Agent** — `build/xrce-agent/MicroXRCEAgent`, built by `just xrce setup`
    (~1.5 min). Without it the 10 `c_xrce_api` tests fail.
  - **Patched qemu** — `build/qemu/bin/qemu-system-arm`, built by
    `just qemu setup-qemu`. Without it the 6 `qemu_patched_binary` tests fail.
- **Workaround (verified 2026-05-27):** `just xrce setup` + `just qemu setup-qemu`
  → all 16 pass.
- **Fix:** either include both in the default-tier `just setup` (note the
  SDK-tier policy in `docs/development/sdk-tiers.md` — patched qemu is a build,
  not a download), or have the tests' skip/remedy messages name the exact recipe
  (some already do).
- Note: a recursive `git pull --recurse-submodules` here hit transient GnuTLS
  errors fetching `third-party/xrce/agent` + `rmw_zenoh`; the Agent build later
  succeeded, so the earlier `build/xrce` absence was a not-yet-built state, not a
  hard network block.

### 184.4 — [RESOLVED] `nuttx_make_e2e` C++ make-fixture link failure (real codegen/link gap)
- [x] `nuttx_make_e2e::nuttx_external_apps_link_into_kernel_binary` fails because
  `just nuttx build-fixtures-make` (the NuttX external-apps → kernel link path,
  not the cmake `build-fixtures`) **fails to link the C++ examples**:
  ```
  .../cpp/service-server/generated/.../add_two_ints.hpp: undefined reference to
    `nros_cpp_deserialize_example_interfaces_srv_add_two_ints_request'
    `nros_cpp_serialize_example_interfaces_srv_add_two_ints_response'
  .../cpp/talker/generated/.../int32.hpp: undefined reference to
    `nros_cpp_publish_std_msgs_msg_int32'
  make[1]: *** [Makefile:217: nuttx] Error 1
  ```
  The generated C++ bindings reference per-type FFI symbols
  (`nros_cpp_{serialize,deserialize,publish}_<pkg>_<msg>`) that are **not provided
  in the make/kernel link** (the cmake `build-fixtures` path links them, but the
  apps/external make path's `libapps.a` does not pull the codegen FFI archive).
- **Actual root cause:** the link hook already exists — each cpp example
  Makefile `-include`s `generated/ffi/extra_libs.mk`, populated by
  `scripts/nuttx/gen-cpp-ffi-crates.py` (builds the per-package
  `nano_ros_cpp_ffi_<pkg>` staticlibs). But that script needs the host
  `nros-codegen` binary, and **`build-fixtures-make` never built it** (the cmake
  `build-fixtures` does, via `nros_cargo_ensure_codegen_c`). After `just clean`
  removed the codegen build, `gen-cpp-ffi` printed "nros-codegen not at …;
  skipping" → empty `extra_libs.mk` → undefined `nros_cpp_*` at kernel link.
- **Fix (RESOLVED, `just/nuttx.just`):** `build-fixtures-make` now sources
  `cargo.sh`, runs `nros_cargo_ensure_codegen_c`, and exports `NROS_CODEGEN` at
  the active profile dir before `stage-external-apps.sh` (mirroring the cmake
  path; `gen-cpp-ffi`/`gen-interfaces` read it). **Verified:** `gen-cpp-ffi`
  emits all 4 FFI staticlibs (incl. `example_interfaces`); a full
  `just nuttx build-fixtures-make` links the kernel ELF clean
  (`NMAKE_EXIT=0`, no `undefined reference`, `extra_libs.mk` = 4 lines).

### 184.5 — [RESOLVED via 184.1]  `test_threadx_linux_cyclonedds_talker_to_native_listener` (missing fixture)
- [x] Fast-fails (0.046 s) the same way the native Cyclone tests did — the
  ThreadX-Linux Cyclone fixture is not built by the clean-room aggregation
  (184.2). Should pass once that fixture is built (Cyclone install present after
  184.1). Folded into 184.2; tracked separately only to confirm after the
  fixture build lands.

### 184.6 — `graph.cpp` (177.36 `ros_discovery_info`) breaks the **zephyr cyclonedds** build (`build-all` RED)
- [x] **FIXED + landed on main** (`4c6ce2520`, 2026-05-27). Before this a fresh
  `build-all` on plain main was **RED** (36 zephyr cyclonedds fixtures); green now.
- 177.36 (`00bc53ef3`) added `packages/dds/nros-rmw-cyclonedds/src/graph.cpp`,
  which `#include "rmw_dds_common_graph.h"` — an **idlc-generated** header. The
  standalone `nros-rmw-cyclonedds/CMakeLists.txt` generates it via
  `NrosRmwCycloneddsTypeSupport` (idlc on `src/idl/rmw_dds_common_graph.idl`).
  The **Zephyr** path (`zephyr/CMakeLists.txt`) compiles the backend by
  `file(GLOB src/*.cpp)` — so it picked up `graph.cpp` but **never ran the
  graph-types codegen nor added its include dir** → all **36** zephyr cyclonedds
  fixtures (rust+c talker/listener/service/action) fail:
  `fatal error: rmw_dds_common_graph.h: No such file or directory`.
- **Fix (landed on the feature branch):** mirror the standalone codegen in the
  zephyr cyclonedds block — pre-set `IDLC_EXECUTABLE` (host idlc; no imported
  `CycloneDDS::ddsc` target on embedded), `include(NrosRmwCycloneddsTypeSupport)`,
  `nros_rmw_cyclonedds_idlc_compile(... rmw_dds_common_graph.idl ...)`, add the
  generated descriptor + register TU to the library sources and the output dir to
  the include path. `build-all` → exit 0 after the fix.
- Note: the earlier `CONFIG_NROS_DOMAIN_ID`-not-found error on the same fixtures
  was a **stale build dir** (cached `kconfig.rs` predating 177.38's symbol) — a
  clean rebuild cleared it and unmasked this graph.cpp gap.

### 184.7 — [RE-VERIFIED 2026-05-27] runtime `test-all` reds were NOT live product bugs
The baseline `test-all` (post-184.6) showed **7 hard failures** + 1 `[SKIPPED]`
(`nuttx_make_e2e`, see 184.4). Re-verification by **building each platform's
release/Cyclone fixtures and re-running the specific test** found that none of
the reproducible reds is a live product bug — they were stale-baseline /
fixture-not-built preconditions (the product fixes had already landed under
177.x), plus one genuine **test**-timing bug:

| Test | Owning phase | Re-verified status (2026-05-27) |
|------|--------------|----------------------------------|
| `large_msg test_zenoh_overflow_detection` | — | **FIXED (test bug, not flake).** The listener only prints `RECV_DONE:` at its internal timeout (every 2048B payload correctly overflows the 1024B shim buffer ⇒ `received=0`, `EXPECTED_COUNT` never reached); the test's `wait_for_output_pattern` raced that exact 15s deadline ⇒ missed the line ⇒ parsed `overflow_drops=0`. Product was correct (`overflow_drops=5`). Fix in `large_msg.rs`: listener `TIMEOUT_SECS` 15→8, test wait 15→25s, `unwrap_or_default`→`expect`. **PASS** (8.2s). |
| `rtos_e2e test_rtos_pubsub_e2e::Nuttx::Rust` | **177.30** | **already fixed; was fixture-missing.** Cold run fell back to `nros-fast-release` (177.8.c reboot loop). After `just nuttx build-fixtures`: **PASS** (45s). |
| `threadx_riscv64_qemu test_threadx_riscv64_cyclonedds_two_qemu_pubsub` | **177.26** | **already fixed; was Cyclone-fixture-missing.** After `just cyclonedds threadx-cross-probe` + `NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=1 just threadx_riscv64 build-fixtures`: **PASS** (5.2s) — the "executor register" blocker is resolved. |
| `native_api test_threadx_linux_cyclonedds_talker_to_native_listener` | **177.26** | **already fixed; was fixture-missing.** After `just cyclonedds setup` + `just threadx_linux build-fixture-extras`: **PASS** (10.1s). |
| `zephyr test_zephyr_dds_{c,cpp,rs}_action_e2e` (3) | **177.2** | **GREEN (not a product gap).** 177.2 closed 2026-05-23; the `k_thread`-ddsrt-port theory was debunked 2026-05-25 (diagnosed against stale `eth_posix` fixtures — Cyclone workers ARE `k_thread`s, `tid in use!` benign). On fresh NSOS fixtures the full `binary(zephyr) & test(dds)` group is **15/15 PASS** incl. all `*_action_e2e`. Clean-room reds were stale-fixture / `zephyr-native-cyclonedds` domain cross-talk (177.33/177.35). Not re-run in this SDK-less env. |

**Takeaway:** the clean-room `test-all` reds are dominated by *fixtures not built
for the specific platform* (the per-test `[SKIPPED]`/fallback messages name the
exact `just … build-fixtures` step). Only `test_zenoh_overflow_detection` was a
real defect — a test-side timing race, now fixed. None are Phase-172 (Group-1)
regressions — Group-1 touches only additive board `from_toml` parsers + the mps2
baremetal example's `nros.toml` + the 184.6 zephyr build fix; none of these test
paths.

## Acceptance
- A clean-room `clean → setup → build-all → build-test-fixtures → test-all` on a
  capacity-adequate host is green with no manual per-tool steps (184.1–184.3),
  OR the required steps are documented in a clean-room runbook and the skip
  messages name them.
- `nuttx_make_e2e` links (184.4).

## Notes
- ~~`build-all` is green~~ **Correction (2026-05-27, see 184.6):** the 177 idlc
  re-resolve harden fixed the idlc-127, but 177.36's `graph.cpp` then broke the
  **zephyr cyclonedds** build (36 fixtures) — `build-all` on plain main is RED
  until the 184.6 fix merges. The other 177 fixes (idlc re-resolve, zenoh fflush
  deadlock, service round-trip retry, `Executor::open` connect-retry) are in
  archived Phase 177 and hold.
- Cyclone test-runner domain isolation (concurrent native_sim nodes sharing DDS
  domains → SPDP cross-talk) is a *separate* concern tracked under 177.33/177.35;
  the 184 native_sim Cyclone fails here were missing-fixture, not concurrency
  (they pass `--test-threads=1` once the fixtures exist).
