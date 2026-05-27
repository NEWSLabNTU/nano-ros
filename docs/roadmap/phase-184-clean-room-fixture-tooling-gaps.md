# Phase 184 — Clean-room fixture / tooling gaps

**Goal:** Close the build/test-infrastructure gaps that make a full clean-room
cycle (`just clean` → `just setup` → `just build-all` → `just build-test-fixtures`
→ `just test-all`) report failing tests even though the product code + build are
green. None of these are product regressions — they are setup/fixture-aggregation
and one codegen-link gap.

**Status:** Open (surfaced 2026-05-27, after Phase 177 was archived).
**Priority:** Medium — they inflate `test-all` red on a fresh clean checkout;
each has a known manual workaround.
**Depends on:** Phase 177 (archived — product fixes incl. the Cyclone idlc
re-resolve harden), Phase 175 (native Cyclone CMake/Corrosion fixtures).

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

### 184.1 — `just clean` removes the Cyclone install; `just setup` doesn't restore it
- [ ] `just clean` (→ `clean-examples clean-fixtures clean-zenohd`) removes
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

### 184.2 — `just build-test-fixtures` does not build the native / ThreadX-Linux Cyclone fixtures
- [ ] The native Cyclone fixtures (`examples/native/<lang>/<case>/build-cyclonedds/`)
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

### 184.3 — Default `just setup` omits the XRCE Agent and the patched qemu
- [ ] On a fresh clean, `test-all` reports ~16 false fails because the default
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

### 184.4 — `nuttx_make_e2e` C++ make-fixture link failure (real codegen/link gap)
- [ ] `nuttx_make_e2e::nuttx_external_apps_link_into_kernel_binary` fails because
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
- This is **not** an infra/setup gap — it is a real link/codegen wiring bug in
  the NuttX external-apps C++ path. Pre-existing (the make fixture has long been
  a `[SKIPPED]` precondition in `test-all`); now it hard-fails when staged.
- **Fix:** wire the per-example C++ codegen FFI object/archive into the NuttX
  external-app (`apps/external/<name>`) link, mirroring how the cmake
  `build-fixtures` path links `nros-cpp` + the example FFI staticlib.

### 184.5 — `test_threadx_linux_cyclonedds_talker_to_native_listener` (missing fixture)
- [ ] Fast-fails (0.046 s) the same way the native Cyclone tests did — the
  ThreadX-Linux Cyclone fixture is not built by the clean-room aggregation
  (184.2). Should pass once that fixture is built (Cyclone install present after
  184.1). Folded into 184.2; tracked separately only to confirm after the
  fixture build lands.

## Acceptance
- A clean-room `clean → setup → build-all → build-test-fixtures → test-all` on a
  capacity-adequate host is green with no manual per-tool steps (184.1–184.3),
  OR the required steps are documented in a clean-room runbook and the skip
  messages name them.
- `nuttx_make_e2e` links (184.4).

## Notes
- The product work is done + verified: `build-all` is green (the 177 idlc
  re-resolve harden eliminated the idlc-127 that previously made `build-all`
  exit 2), and the 177 zenoh fixes (fflush deadlock, service round-trip retry,
  `Executor::open` connect-retry) are in archived Phase 177.
- Cyclone test-runner domain isolation (concurrent native_sim nodes sharing DDS
  domains → SPDP cross-talk) is a *separate* concern tracked under 177.33/177.35;
  the 184 native_sim Cyclone fails here were missing-fixture, not concurrency
  (they pass `--test-threads=1` once the fixtures exist).
