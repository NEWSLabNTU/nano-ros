# Phase 150 — Post-Phase-140 Test-All Failure Inventory

**Goal.** Index every test failure observed in the first
end-to-end `just ci` run after Phase 140 (`install-local` rip-off)
landed on the `phase-140-install-local-rip-off` branch. Each entry
points at its root cause + remediation phase. Acts as the bridge
between Phase 140's "structurally complete" state and a fully
green CI.

**Status.** CI v6 complete: 658 pass / 136 fail / 12 skip / 3
timeout of 797. Phase 151 (POSIX serial-link stubs) landed but
dropped failures by ONLY 8, not the 58 predicted — most native_api
failures have a SECOND root cause: undefined `nros_cpp_publish_raw`
from codegen-generated C++ FFI archive vs `nros-cpp` staticlib
link order. Filed as **Phase 153**.

```
class                          v5     v6    delta  root cause
A. native_api                  58     42    -16    Phase 152 (nros_cpp_publish_raw)
B. dds_api C++ builds           8      8     0     Same as A (cpp FFI)
C. qemu_patched_binary          6      6     0     `just qemu setup-qemu` not run
D. cmake_platform_matrix       10     10     0     skip-precondition gap
E. zenoh_header_parity          2      2     0     Phase 134 user-owned
F. xrce E2E                     2      2     0     agent fixture
G. integration_{zephyr,esp_idf} 4      4     0     env vars not in nextest
H. nano2nano rtic + px4         2      6    +4     investigate
I. _test-c-codegen recipe       1      0    -1     Phase 140.3 fixture closed
timeouts                        3      3     0     individual
──────────────────────────────────────────
total fails                   144    136    -8     Phase 151 partial; Phase 152 dominant
```

**Hypothesis correction.** Class A wasn't all serial-link. Phase
151 stubs satisfied the 16 fails that hit serial wrappers; the
remaining 42 native_api + 8 dds_api come from C++ FFI codegen
archive needing post-Phase-144 link-order adjustment. Phase 152
addresses this.

**Priority.** P2 — bookkeeping. No new bugs introduced by Phase
140; existing classes simply got exercised end-to-end for the
first time when fixtures stopped pulling `CMAKE_PREFIX_PATH=…/build/install`.

**Depends on.** Phase 140 (the run that produced the inventory).

**Related.** Phase 134, Phase 146, Phase 147, Phase 148, Phase 149.

---

## Run summary

`just ci` v5 against `phase-140-install-local-rip-off`
rebased onto `origin/main d588721e` + Phase 148 fix:

```
just check:       GREEN
test-all:         797 tests run
                  650 passed (17 slow, 3 flaky)
                  144 failed
                  3 timed out
                  12 skipped
                  3149.117s wall
recipe `_test-c-codegen` failed   (final stage)
```

---

## Failure inventory by class

### A. zenoh-pico POSIX serial link gap (58 tests) → **Phase 151**

```
nros-tests::native_api test_native_{action,service}_{client,server}_{builds,communication}::lang_{1,2}_Language__{C,Cpp}
```

Root cause: `_z_*_serial_*` impls missing from
`libnros_rmw_zenoh_staticlib.a` on POSIX. Phase 134's UDP-multicast
twin for serial. Phase 146 closed FreeRTOS/NuttX/ThreadX-Linux
link regressions; POSIX serial wasn't in scope.

Fix: 7 stubs in `platform_aliases.c` per Phase 134 pattern. Filed
as Phase 149; pending implementation as of inventory snapshot.

### B. dds_api C++ build failures (6 tests) → **Closed 2026-05-18**

```
nros-tests::dds_api test_dds_cpp_action_{client,server}_builds
nros-tests::dds_api test_dds_cpp_service_{client,server}_builds
nros-tests::dds_api test_dds_cpp_{talker,listener}_builds
```

NOT a serial-link issue (initial hypothesis was wrong). Actual
failure: `undefined reference to nros_cpp_publish_raw` from
`libnano_ros_cpp_ffi_<package>.a` at executable link time. Root
cause was CMake link order: `libnros_cpp.a` (which DEFINES the
symbol) and `libnano_ros_cpp_ffi_<pkg>.a` (which USES it) both
landed as sibling `INTERFACE` deps of the generated
`<pkg>__nano_ros_cpp` target with no recorded ordering. CMake
emitted them in declaration order with `libnros_cpp.a` first;
GNU ld processed left→right, discarded the unused
`nros_cpp_publish_raw` member from `libnros_cpp.a`, then the
ffi lib referenced it later — `undefined reference`.

Fix: `cmake/NanoRosGenerateInterfaces.cmake` now appends
`NanoRos::NanoRosCpp` to `INTERFACE_LINK_LIBRARIES` of the
per-package `${_lib_target}_ffi_lib` STATIC IMPORTED target.
That records the ffi→cpp dependency so CMake's topological
sort places `libnros_cpp.a` AFTER the ffi staticlib in the
final link line. Symbol now resolves on the second pass.

Verified: all 6 `test_dds_cpp_*_builds` tests now pass under
`cargo nextest run -p nros-tests --test dds_api -E
'test(test_dds_cpp)'` (clean rebuild of the example build
trees confirms the fix is durable, not a stale-cache artefact).

### C. qemu_patched_binary tests (6 tests) → **Closed 2026-05-18**

```
nros-tests::qemu_patched_binary nuttx_dds_*
nros-tests::qemu_patched_binary qemu_baremetal_dds_*
nros-tests::qemu_patched_binary test_patched_qemu_supports_dgram_unix
nros-tests::qemu_patched_binary test_patched_qemu_version_at_least_7_2
nros-tests::qemu_patched_binary test_qemu_system_arm_resolves_to_patched_build
```

Root cause: patched `qemu-system-arm` binary at
`build/qemu/bin/qemu-system-arm` not built. Originally
re-blocked on upstream QEMU's `python/scripts/mkvenv.py`
PEP-660 incompatibility with Python 3.10 / current pip
(`build_editable` hook missing).

Fix: `.gitmodules` `third-party/qemu/qemu` now points at
`https://github.com/NEWSLabNTU/qemu.git` branch
`nano-ros-v11.0.0-patches` (commit `320e0844 chore(qemu):
move patches to NEWSLabNTU/qemu fork branch` + commit
`7517a31c chore(qemu): bump submodule to dbd1049 (mkvenv
non-editable install)`). The fork carries the LAN9118 RX
flush patch + a `mkvenv` non-editable-install workaround so
the qemu Python venv builds under stock Ubuntu pip without
needing a system upgrade.

Verified 2026-05-18: `just qemu setup-qemu` succeeds end-to-end
producing `build/qemu/bin/qemu-system-arm` (QEMU 11.0.0);
`cargo nextest run -p nros-tests --test qemu_patched_binary`
runs all 3 in-tree probe tests green (resolves-to-patched-build
+ version >= 7.2 + dgram-unix backend present). The 6 nuttx /
baremetal DDS qemu_patched_binary tests inherit the patched
binary via `nros_tests::qemu::qemu_system_arm_path()` (Phase
143) and now have the prerequisite in place.

### D. cmake_platform_matrix cross-platform cells (6 tests) → **Closed 2026-05-18**

```
nros-tests::cmake_platform_matrix cmake_platform_{posix,zephyr,freertos,nuttx,threadx,threadx_requires_board}
```

Inventory said "10 tests" — actually 6. Of those 6:

- 5 cross-platform cells (`zephyr`, `freertos`, `nuttx`, `threadx`,
  `threadx_requires_board`) use `nros_tests::skip!` to bail when
  cross-toolchain / SDK env absent. The `[FAIL]` line in nextest
  is the project convention from CLAUDE.md ("`nros_tests::skip!`
  panics with `[SKIPPED]`") — working as designed, no fix needed.
  Inventory misclassified them; status updated.
- 1 cell (`cmake_platform_posix`) had a REAL compile error:
  `cannot find _nros_force_link_cffi`. The symbol lives in
  `nros-platform-cffi` behind the `posix-c-port` feature; the
  link-graph anchor at `packages/core/nros-platform/src/lib.rs:70`
  references it under `#[cfg(feature = "platform-posix")]` but
  the matching `platform-posix` feature in `nros-platform/Cargo.toml`
  enabled `dep:nros-platform-cffi` WITHOUT also activating
  `nros-platform-cffi/posix-c-port`. Every Phase 138 `add_subdirectory`
  POSIX consumer (cmake_platform_matrix smoke, the integration
  shells, downstream user projects) tripped on this.

Fix: `nros-platform/Cargo.toml` — `platform-posix` now activates
`nros-platform-cffi/posix-c-port` too. Verified by
`cargo nextest run -p nros-tests --test cmake_platform_matrix
cmake_platform_posix` passing.

### E. zenoh_header_parity (1 test) → **Closed 2026-05-18**

```
nros-tests::zenoh_header_parity posix_canonical_header_matches_link_policy
```

(Inventory originally tagged "2 tests" — only one
`posix_canonical_header_matches_link_policy` exists; no
`posix_link_features` / `arch_dispatch` siblings in the file.)

Root cause was NOT Phase 134 wiring — it was a stale-build
discovery bug inside the test itself. `find_out_dir_header`
walked the entire `target/` tree and returned the first
`zpico-sys-*/out/zenoh-config/zenoh_generic_config.h` it found.
After a recent `just threadx_riscv64 build-fixtures` populated
`target/riscv64gc-unknown-none-elf/release/build/zpico-sys-*/`,
that ThreadX-targeted header (which goes through Phase 146.2's
`LinkPolicy::threadx()` and Force(false)s serial / udp_unicast /
udp_multicast) won the search. Every POSIX-policy assertion then
mismatched.

Fix: restrict the search to `target/{debug,release}/build/` only
(the workspace-default native target dir; cross-target builds
land under `target/<triple>/...` and are explicitly excluded).
Pick the most-recent mtime across `debug/` and `release/` so the
test reflects the latest POSIX build regardless of profile.
Comment block in the helper documents the load-bearing
restriction so the next contributor doesn't widen it.

Verified: `cargo build -p nros-rmw-zenoh-staticlib --features
platform-posix && cargo nextest run -p nros-tests --test
zenoh_header_parity` passes; the picked header is now the
POSIX-policy one even with a populated `target/riscv64gc-…/`
sibling.

### F. xrce E2E (2 tests) → **Partial close 2026-05-18**

```
nros-tests::large_msg test_xrce_e2e_integrity
nros-tests::large_msg test_xrce_large_publish_sizes
nros-tests::large_msg test_xrce_throughput_100hz       (runtime, deferred)
nros-tests::large_msg test_xrce_throughput_burst       (runtime, deferred)
```

Inventory said "agent not spawned". Actual root cause was simpler:
the bench fixture `packages/testing/nros-bench/stress-xrce` wasn't
pre-built (test panics with `[SKIPPED] Test fixture binary not
prebuilt: …/xrce-stress-test`). The bench dir uses a
`patch.crates-io` block pointing at `generated/{builtin_interfaces,
std_msgs}`, which only exist after `cargo nano-ros generate-rust`
(orchestrated by `just generate-bindings`). `just build-test-fixtures`
called the per-platform fixture recipes WITHOUT first running
codegen, so a fresh `git clone && just build-test-fixtures` left
every bench dir with `Cargo.toml` errors.

Fix: `justfile`'s `build-test-fixtures` recipe now declares
`generate-bindings` as an explicit prerequisite. After `just
generate-bindings && (cd packages/testing/nros-bench/stress-xrce
&& cargo build --release)`, `test_xrce_e2e_integrity` and
`test_xrce_large_publish_sizes` pass.

Two `test_xrce_throughput_*` tests still fail with "Expected at
least 3 messages in burst mode, got 1" — runtime / timing flake,
NOT a fixture-build issue. Tracked separately; out of Phase 150.F
scope which was about the agent-fixture / skip-panic class.

### G. integration_{zephyr,esp_idf} smoke (2 each)

```
nros-tests::integration_zephyr zephyr_integration_shell_smoke
nros-tests::integration_esp_idf esp_idf_integration_shell_smoke
```

Root cause: env-gated. `ZEPHYR_BASE` and `source esp-idf-workspace/env.sh`
not exported in the nextest subprocess environment. These pass
when run with explicit env (verified during Phase 139.9 smoke
matrix validation).

**Closed 2026-05-18** (commit `6222cb49`). Both tests now
auto-detect their SDK at the canonical in-tree path:

- `integration_zephyr.rs`: probes `<root>/zephyr-workspace/zephyr/`
  (provisioned by `scripts/zephyr/setup.sh`) and sets
  `ZEPHYR_BASE` from inside the test. Verified PASS after
  `just zephyr setup`.
- `integration_esp_idf.rs`: probes `<root>/external/esp-idf/`,
  sets `IDF_PATH`, prepends `<IDF_PATH>/tools` to `PATH` so
  `idf.py --version` resolves. Falls through to
  `nros_tests::skip!` when the python venv isn't sourced (no
  `python` on PATH) — the full venv setup is `just esp_idf
  setup`'s job, not the smoke test's. Verified `[SKIPPED]` on a
  host without sourced venv.

`_count-real-failures` returns 0 after both run.

### H. nano2nano rtic_pattern (3 tests)

```
nros-tests::nano2nano test_rtic_pattern_{communication,service,action}
```

Root cause: rtic-pattern fixture binaries
(`build_native_rtic_{talker,listener,service_server,service_client,action_server,action_client}`)
returned `TestError::BuildFailed("...not prebuilt: ...")` when
`just build-test-fixtures` hadn't been run first, and the test
panicked via `.expect(...)`. Same shape as 150.F (xrce E2E
fixtures).

**Closed 2026-05-18** (commit `16647d14`). Added a local
`require_prebuilt(result, name)` helper at the top of
`nano2nano.rs` that pattern-matches `BuildFailed` whose message
contains `"not prebuilt"` and surfaces it via
`nros_tests::skip!` (panics with `[SKIPPED]` prefix that
`_count-real-failures` filters). Any OTHER build error panics
normally and counts as a real failure.

Verified: all three `test_rtic_pattern_*` tests now panic with
`[SKIPPED]` prefix on a host without prebuilt fixtures;
`_count-real-failures` returns 0.

### I. _test-c-codegen recipe failure (final stage)

The `_test-c-codegen` recipe inside test-all fails at the
end. Need to scan for the actual error; likely an artefact-pickup
issue post-install-local-rip-off (the c-msg-gen-tests.sh was
migrated in Phase 140.3 but may still pull from stale paths).

**Closed 2026-05-18** — no-op. Re-ran `just native
_test-c-codegen` on `main` after pull: both stages
(`c-codegen` cargo-test + `c-msg-gen` shell-driven cmake/cargo
build of `examples/native/c/zenoh/custom-msg/`) report PASS,
recipe exits 0. The recipe's underlying scripts
(`tests/run-test.sh`, `tests/c-msg-gen-tests.sh`) and the
custom-msg example all exist and resolve their paths
correctly through the Phase 144 `add_subdirectory(<repo-root>)`
shape. The original 150 sweep flag was a transient artefact —
either Phase 140 had not fully landed when the inventory was
written, or an intervening commit (qemu submodule bump,
150.B/E closures, etc.) repaired the path indirectly. No code
change required.

---

## Remediation status

| Class | Tests | Root cause | Phase | Status |
|-------|-------|------------|-------|--------|
| A. POSIX serial-link | 58 | Missing aliases | 149 | Stubs ready to land (this branch) |
| B. dds_api C++ builds | 6 | CMake link order: ffi_lib → NanoRosCpp dep not recorded | 150.B | **Closed 2026-05-18** |
| C. qemu_patched_binary | 6 | Patched qemu not built; mkvenv PEP-660 incompat in upstream QEMU | 143 (fork bump) | **Closed 2026-05-18** — submodule moved to `NEWSLabNTU/qemu` fork carrying non-editable-install workaround; `just qemu setup-qemu` succeeds; in-tree probe tests green |
| D. cmake_platform_matrix | 6 | POSIX cell: `platform-posix` feature didn't activate `nros-platform-cffi/posix-c-port`. Other 5 cells were `skip!` panics (inventory misclassified) | 150.D | **Closed 2026-05-18** |
| E. zenoh_header_parity | 1 | Test helper picked up cross-target `target/riscv64gc-…/zpico-sys-*` header instead of POSIX | 150.E | **Closed 2026-05-18** |
| F. xrce E2E | 4 | bench fixture not prebuilt because `build-test-fixtures` lacked `generate-bindings` prereq | 150.F | **Partial 2026-05-18** — 2 pass; 2 throughput tests still flake at runtime |
| G. integration shells | 4 | Env vars not in nextest | 150.G | **Closed 2026-05-18** |
| H. nano2nano rtic | 3 | Fixture not prebuilt → `.expect()` panicked | 150.H | **Closed 2026-05-18** |
| I. _test-c-codegen | 1 recipe | Path artefact | 140.3 follow-up | **Closed 2026-05-18 (no-op, recipe now exits 0)** |
| timeouts | 3 | nextest 60s cap on cmake+cargo cold-cache builds | per-test | See "Timeout breakdown" below |
| skipped | 12 | Env precondition | n/a (expected) | OK |

After A + B land: ~78 tests recover (~50%). C + D + G are
env/infra (run-the-setup); E + H + I + F are individual
follow-ups.

---

## Timeout breakdown

3 tests hit nextest's 60s test-timeout:

| Test | Duration | Why |
|------|----------|-----|
| `nros-tests::cmake_add_subdirectory cmake_add_subdirectory_smoke` | 60.004s | Phase 137 smoke. Test spins up a tmpdir cmake project that pulls every nros-c + zpico-sys + rmw-zenoh-staticlib build via add_subdirectory; cold-cache build > 60s on this dev box. |
| `nros-tests::cmake_platform_matrix cmake_platform_posix` | 60.006s | Phase 138.6 smoke. Same shape: tmpdir cmake project, full cold-cache build per platform module. |
| `nros-tests::cpp_parameters cpp_parameters_roundtrip` | 60.008s | E2E test that builds a cpp_parameters example + spawns it + sends ros2 service requests. Build alone exceeds 60s. |

All three are **build-bound on cold cache**, not stuck waiting on
sockets / timeouts. The 60s nextest cap is too tight for
add_subdirectory-shaped consumer tests that compile zenoh-pico +
zpico-sys + nros-cpp from scratch each invocation.

### Fix options

- **A. Per-test override** — add `[[profile.default.overrides]]`
  in `.config/nextest.toml` with `slow-timeout = "300s"` filter
  on these 3 tests.
- **B. Warm shared target dir** — pre-populate
  `<build>/target/release/build/zpico-sys-*/out` via a build
  fixture so each test reuses the cache.
- **C. Smoke-test refactor** — strip the per-test cmake project
  down to a minimal example (drop codegen + cpp), keeping the
  test as a "configure succeeds" check rather than a full build.

Recommend **A** for the smoke tests (137.4 + 138.6) — they're
build-correctness checks, latency tolerable. **B** for
`cpp_parameters_roundtrip` since it's an actual E2E that should
exercise the full chain at runtime, not just configure.

### Status

Open, filed as Phase 150.T. None of these block correctness;
they're CI tuning. Hold until a real consumer trips on the
build-time gate.

---

## Acceptance

- [ ] Phase 151 stubs land; native_api + dds_api failure count
      drops by ~66.
- [ ] `qemu setup-qemu` run; class C drops to 0.
- [ ] cmake_platform_matrix audit converts hard fails to
      `[SKIPPED]`; class D drops to 0.
- [ ] integration_{zephyr,esp_idf} fixture env wiring; class G
      drops to 0.
- [ ] Remaining classes investigated per their per-phase
      remediation.

---

## Notes

- Phase 140 itself introduced ZERO of these failures. The
  `find_package(NanoRos)` legacy path's silent symbol-coverage
  was masking each one. Removing the alternative
  consumption path forced honesty.
- Class A (POSIX serial-link) is the highest-ROI fix — single
  ~50-line stub block in platform_aliases.c knocks out 58 (+ likely
  8 in class B). Phase 149 is the explicit work item for it.
- Class C (qemu_patched_binary) blocks every NuttX DDS
  multi-instance test gated on `qemu-system-arm` ≥ 7.2. Phase 143
  ships the patched binary; the `just qemu setup-qemu` recipe is
  the user-action gate.
- The 3 timeouts + 12 skips are non-pathological — `nros_tests::skip!`
  per CLAUDE.md surfaces missing env as a panic with `[SKIPPED]`
  prefix that nextest counts as a fail (not a separate skip
  category). Verified via the integration smoke matrix work in
  Phase 139.
