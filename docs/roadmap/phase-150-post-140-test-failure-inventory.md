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

### C. qemu_patched_binary tests (6 tests)

```
nros-tests::qemu_patched_binary nuttx_dds_*
nros-tests::qemu_patched_binary qemu_baremetal_dds_*
```

Root cause: patched `qemu-system-arm` binary at
`build/qemu/bin/qemu-system-arm` not built. `just qemu setup-qemu`
not run; depends on Phase 143 (qemu-system-arm unification) +
distro qemu < 7.2.

Fix: run `just qemu setup-qemu` (one-time, ~10 min build) OR
upgrade system qemu via Canonical PPA. Pre-existing per
`just doctor` warning.

### D. cmake_platform_matrix cross-platform cells (10 tests)

```
nros-tests::cmake_platform_matrix cmake_platform_*
```

Root cause: cross-platform smoke matrix expects
`[SKIPPED]` cleanly when toolchain absent. Some skips evaluating
as test failures instead of `nros_tests::skip!` panics. Possibly
Phase 138.6 (cmake_platform_matrix) test infra needs revision.

Fix: audit `cmake_platform_matrix.rs` for missing `skip!`
preconditions; convert hard failures to `[SKIPPED]` per CLAUDE.md
rule.

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

### F. xrce E2E (2 tests)

```
nros-tests::xrce xrce_e2e_*
```

Likely XRCE agent connection / config issue. XRCE agent was built
(`build/xrce-agent/MicroXRCEAgent` per `just xrce setup`); test
may need agent process spawned in fixture. Possibly stale.

### G. integration_{zephyr,esp_idf} smoke (2 each)

```
nros-tests::integration_zephyr zephyr_integration_shell_smoke
nros-tests::integration_esp_idf esp_idf_integration_shell_smoke
```

Root cause: env-gated. `ZEPHYR_BASE` and `source esp-idf-workspace/env.sh`
not exported in the nextest subprocess environment. These pass
when run with explicit env (verified during Phase 139.9 smoke
matrix validation).

Fix: nextest config wires env vars per-test OR fixtures source
the right env scripts. Phase 139 follow-up.

### H. nano2nano rtic_pattern (2 tests)

```
nros-tests::nano2nano test_rtic_pattern_{communication,service}
```

Root cause: needs investigation. Possibly RTIC-pattern example
build chain affected by Phase 144 add_subdirectory migration's
rtic-related path changes.

### I. _test-c-codegen recipe failure (final stage)

The `_test-c-codegen` recipe inside test-all fails at the
end. Need to scan for the actual error; likely an artefact-pickup
issue post-install-local-rip-off (the c-msg-gen-tests.sh was
migrated in Phase 140.3 but may still pull from stale paths).

---

## Remediation status

| Class | Tests | Root cause | Phase | Status |
|-------|-------|------------|-------|--------|
| A. POSIX serial-link | 58 | Missing aliases | 149 | Stubs ready to land (this branch) |
| B. dds_api C++ builds | 6 | CMake link order: ffi_lib → NanoRosCpp dep not recorded | 150.B | **Closed 2026-05-18** |
| C. qemu_patched_binary | 6 | Patched qemu not built | 143 | Run `just qemu setup-qemu` |
| D. cmake_platform_matrix | 10 | Skip-precondition gap | 138.6 follow-up | Filed as TODO |
| E. zenoh_header_parity | 1 | Test helper picked up cross-target `target/riscv64gc-…/zpico-sys-*` header instead of POSIX | 150.E | **Closed 2026-05-18** |
| F. xrce E2E | 2 | Agent not spawned | XRCE fixture | TODO |
| G. integration shells | 4 | Env vars not in nextest | 139.9 follow-up | TODO |
| H. nano2nano rtic | 2 | Investigate | 144 follow-up | TODO |
| I. _test-c-codegen | 1 recipe | Path artefact | 140.3 follow-up | Investigate |
| timeouts | 3 | Unrelated | per-test | Investigate |
| skipped | 12 | Env precondition | n/a (expected) | OK |

After A + B land: ~78 tests recover (~50%). C + D + G are
env/infra (run-the-setup); E + H + I + F are individual
follow-ups.

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
