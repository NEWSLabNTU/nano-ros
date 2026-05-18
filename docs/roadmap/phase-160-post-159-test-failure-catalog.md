# Phase 160 — Post-Phase-159 Test-All Failure Catalog

**Goal.** Catalog every distinct `just test-all` failure remaining after
Phases 154 / 155 / 156 / 159 + the Phase 140 install-local rip-off
follow-ups landed. Acts as the index a future per-cluster phase
inherits from. Phase 150 (post-Phase-140 inventory) served the same
role for the previous baseline and is now archived; this doc is its
successor.

**Status.** Inventory captured 2026-05-19 from a clean
`just test-all` run with fresh `just build-test-fixtures` (commit
`ba43f2da` HEAD).

**Run summary.** 803 tests / 740 pass / 63 fail / 12 skip / 37 slow
/ 3 flaky. Fail count down from 189 (cf. Phase 150 v6 inventory) →
110 (mid-session) → 63 here.

**Priority.** Medium — no test is gating a release; clusters split
naturally along subsystem boundaries that map onto independent
follow-up phases.

## Failure inventory by cluster

### A. Zephyr XRCE C/C++ (11 tests) → **needs new phase**

```
test_zephyr_xrce_c_talker_listener
test_zephyr_xrce_cpp_action_client_boots
test_zephyr_xrce_cpp_action_e2e
test_zephyr_xrce_cpp_action_server_boots
test_zephyr_xrce_cpp_listener_boots
test_zephyr_xrce_cpp_service_client_boots
test_zephyr_xrce_cpp_service_e2e
test_zephyr_xrce_cpp_service_server_boots
test_zephyr_xrce_cpp_talker_boots
test_zephyr_xrce_cpp_talker_listener
```

**Symptom.** Boot logs reach `nros_support_init_named(...) -> -3`
(`NROS_RET_INVALID_ARGUMENT`) before any communication starts.

**Hypothesis.** Locator parsing rejects the
`CONFIG_NROS_XRCE_AGENT_ADDR ":" STRINGIFY(CONFIG_NROS_XRCE_AGENT_PORT)`
string the Kconfig defaults assemble — either the
`NROS_RMW_XRCE_AGENT_PORT` int macro fails `STRINGIFY` →
empty/invalid token, or the locator parser in
`packages/xrce/nros-rmw-xrce-cffi/src/lib.rs` rejects shape XRCE
expects (no protocol prefix vs `udp/...`).

**Repro.**
```
cargo nextest run -p nros-tests --release \
  -E 'test(test_zephyr_xrce_c_talker_listener)' --no-capture
```

### B. Zephyr Cyclone-A9 DDS Rust (4 tests) → **needs new phase**

```
test_zephyr_dds_rust_action_a9_e2e
test_zephyr_dds_rust_async_service_a9_e2e
test_zephyr_dds_rust_service_a9_e2e
test_zephyr_dds_rust_talker_to_listener_a9_e2e
```

**Hypothesis.** `qemu_cortex_a9` board target — likely Cortex-A9
Rust patch (`scripts/zephyr/cortex-a9-rust-patch.sh`) or
dust-dds-on-zephyr stack regression. Sibling `native_sim` DDS
tests pass.

### C. Zephyr cross-host bridge E2E (8 tests) → **needs new phase**

```
test_bidirectional_native_zephyr_e2e
test_native_server_zephyr_client
test_native_talker_to_zephyr_cpp_listener
test_native_to_zephyr_e2e
test_zephyr_cpp_action_server_to_client_e2e
test_zephyr_cpp_service_server_to_client_e2e
test_zephyr_cpp_talker_to_listener_e2e
test_zephyr_cpp_talker_to_native_listener
test_zephyr_action_e2e
test_zephyr_talker_to_listener_e2e
test_zephyr_to_native_e2e
```

**Hypothesis.** Same root cause as A — Zephyr-side XRCE/zenoh
session init fails (`-3`), bridge counterpart times out waiting.
Likely closes when A closes.

### D. NuttX C/C++ rtos_e2e (6 tests) → **Phase 140 follow-up**

```
test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C
test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_3_Lang__Cpp
test_rtos_pubsub_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C
test_rtos_pubsub_e2e::platform_2_Platform__Nuttx::lang_3_Lang__Cpp
test_rtos_service_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C
test_rtos_service_e2e::platform_2_Platform__Nuttx::lang_3_Lang__Cpp
```

**Root cause.** `just nuttx build-fixtures` skips C/C++ cmake
examples (`↷ nuttx C/C++ cmake examples skipped — see Phase 140
follow-up`) because `nros-c` needs `-Z build-std` to emit
`nros_config_generated.h` for tier-3 NuttX target, AND the alias
TU's vendor-mode dependency-closure differs.

**Fix path.** Extract `nros_config_generated.h` size-probe to a
standalone codegen step that runs on the host and writes a header
the cmake project consumes verbatim. Tracked in this phase as
**160.D**.

### E. ESP32 emulator (3 tests) → **env-gated**

```
test_esp32_talker_listener_e2e
test_esp32_to_native
test_native_to_esp32
```

**Root cause.** ESP-IDF + Rust ESP32 toolchain (espflash, riscv32imc
nightly) not bootstrapped on CI host; tests fail rather than
`[SKIPPED]`. Phase 150 G covered the env-skip pattern but ESP32
emulator wasn't included. Tracked as **160.E** (extend
`nros_tests::skip!` precondition checks).

### F. QEMU bare-metal RTIC + serial (5 tests)

```
test_qemu_rtic_action_e2e
test_qemu_rtic_mixed_priority_pubsub_e2e
test_qemu_rtic_pubsub_e2e
test_qemu_rtic_service_e2e
test_qemu_serial_pubsub_e2e
test_qemu_zenoh_large_publish
```

**Hypothesis.** Either:
- Phase 132 (descoped) cmsdk-uart IRQ — serial pubsub blocked on
  init-handshake regression.
- Phase 141 (active) wake-callback cortex-m3 plumbing — RTIC
  scheduling change may have surfaced a regression.

Triage one to confirm + file under whichever phase fits.

### G. Cmake platform matrix (4 tests) → **fixture build gap**

```
cmake_platform_freertos
cmake_platform_nuttx
cmake_platform_threadx
cmake_platform_zephyr
```

**Root cause.** Each test compiles a minimal C example for the
named platform via the top-level `add_subdirectory(<repo>)`
shape. The four embedded targets need cross toolchains + per-
RTOS prerequisite env (FREERTOS_DIR, NUTTX_DIR, THREADX_DIR,
Zephyr workspace). When env is present they should pass; when
not, the matrix entries should `[SKIPPED]`. Convert hard fails to
preconditioned `skip!` panics. Tracked as **160.G**.

### H. nano2nano + cross-RMW bridges (4 tests)

```
bridge_xrce_to_dds_starts_and_opens_both_sessions
bridge_zenoh_to_dds_starts_and_opens_both_sessions
test_c_rust_pubsub_interop
test_xrce_action_fibonacci
test_xrce_throughput_100hz
test_xrce_throughput_burst
```

**Hypothesis.** Single XRCE-agent flake or shared `g_session`
singleton issue Phase 156 doc flagged at the end. Bridge tests
open TWO RMW backends in same process; XRCE's process-global
state may collide with zenoh.

### I. ThreadX-Linux rtos_e2e (3 tests)

```
test_rtos_action_e2e::platform_3_Platform__ThreadxLinux::lang_1_Lang__Rust
test_rtos_pubsub_e2e::platform_3_Platform__ThreadxLinux::lang_1_Lang__Rust
test_rtos_service_e2e::platform_3_Platform__ThreadxLinux::lang_1_Lang__Rust
```

**Hypothesis.** Phase 154 / 155.A platform-aliases ABI work
covered ThreadX-Linux, but a recent fixture rebuild may have
re-introduced staleness. Re-run fixture build + verify before
filing.

### J. RV64 C pubsub (1 test)

```
test_rtos_pubsub_e2e::platform_4_Platform__ThreadxRiscv64::lang_2_Lang__C
```

C example only; Rust variant passes after Phase 120.3 close.
Fixture missing per Phase 140 follow-up (same family as D).

### K. NuttX DDS + ThreadX-Linux DDS (2 tests)

```
test_nuttx_dds_rust_talker_to_listener_e2e
test_threadx_linux_dds_rust_talker_to_listener_e2e
```

Per-platform dust-dds bring-up. NuttX side may share root cause
with B (Zephyr A9 DDS).

### L. Native + misc (8 tests)

```
test_c_xrce_listener_builds
test_c_xrce_listener_starts
test_c_xrce_talker_builds
test_c_xrce_talker_listener_communication
test_c_xrce_talker_starts
test_native_talker_listener_communication::lang_1_Language__C
test_zenoh_overflow_detection
test_qos_reliable_delivery  (and other QoS tests)
```

c_xrce_api family + native_talker_listener_communication C variant
+ qos / overflow scattered fails. Each one-off — investigate
individually.

### M. Integration shells (2 tests)

```
esp_idf_integration_shell_smoke
nuttx_external_apps_link_into_kernel_binary
px4_integration_template_smoke
```

Phase 139 / 157 integration-shell smoke tests. Each gated on
having that vendor SDK staged; convert to `skip!` when env
absent.

## Remediation status

| Cluster | Tests | Hypothesis | Phase hook |
|---------|-------|------------|------------|
| A. Zephyr XRCE C/C++ | 11 | locator parsing `nros_support_init -> -3` | New (160.A) |
| B. Zephyr Cortex-A9 DDS Rust | 4 | dust-dds-on-A9 / Cortex-A9 Rust patch | New (160.B) |
| C. Zephyr cross-host bridge | 8 | cascades from A | Closes with A |
| D. NuttX C/C++ rtos_e2e | 6 | fixture skip (Phase 140) | 160.D codegen-header split |
| E. ESP32 emulator | 3 | env precondition not enforced | 160.E `skip!` wiring |
| F. RTIC + serial bare-metal | 5 | Phase 132 / 141 RTIC regression | Triage → 132 or 141 |
| G. cmake_platform_matrix cross | 4 | env precondition not enforced | 160.G `skip!` wiring |
| H. nano2nano + bridges | 4 | XRCE `g_session` process-globals | Phase 156 follow-up |
| I. ThreadX-Linux rtos_e2e | 3 | fixture staleness | Rebuild + verify |
| J. RV64 C pubsub | 1 | fixture skip (Phase 140) | Closes with D |
| K. NuttX + ThreadX-Linux DDS | 2 | per-platform dust-dds bring-up | Phase 117-adjacent |
| L. Native + c_xrce + qos | 8 | scattered, one-offs | Per-test triage |
| M. Integration shells | 3 | env precondition not enforced | 160.M `skip!` wiring |
| skipped | 12 | env (expected) | OK |
| **total** | **66** unique (63 + 3 retries-only) | | |

## Work items

- [ ] **160.D — NuttX C/C++ rtos_e2e fixture path.** Extract
      `nros_config_generated.h` size probe into a standalone host
      codegen step so tier-3 cross builds can consume the header
      without `-Z build-std`-ing nros-c. Closes 6 NuttX C/C++ + 1
      RV64 C fail.
- [ ] **160.E + 160.G + 160.M — env-precondition `skip!` wiring.**
      Each cluster has clear env gates (ESP_IDF_DIR, cross
      toolchains, vendor SDK staging); convert hard fails to
      `nros_tests::skip!` so missing env reports `[SKIPPED]` not
      `FAIL`. Closes 10 tests on hosts without those SDKs.
- [ ] **160.A — Zephyr XRCE locator parsing.** Trace
      `nros_support_init_named` → `transport_error_to_ret` ladder
      for the `-3` exit; print the input locator + parse stage.
      Likely a Kconfig macro expansion bug or XRCE locator format
      drift. Closes 11 directly + cascades to 8 in C.
- [ ] **160.B — Zephyr Cortex-A9 DDS bring-up triage.** Re-run
      `just zephyr build-fixtures NROS_ZEPHYR_PRISTINE=always` +
      check Cortex-A9 Rust patch is current.
- [ ] **160.H — XRCE g_session collision audit.** Bridge tests
      open zenoh + XRCE in the same process; XRCE's process-global
      session in `zpico.c` (per Phase 156 closing note) likely
      collides. Move to per-Executor session OR document the
      single-RMW constraint.

## Acceptance

- [ ] 160.D lands; NuttX rtos_e2e 9/9 PASS (Rust passes today, C +
      C++ blocked on fixture).
- [ ] 160.E/G/M land; the 10 env-precondition tests report
      `[SKIPPED]` on hosts without the SDK, `PASS` when env is
      present.
- [ ] 160.A lands; Zephyr XRCE C/C++ 11/11 PASS.
- [ ] Remaining clusters investigated per their per-phase hook.

## Notes

- This phase is an INDEX, not implementation work. Each cluster
  spins off its own remediation phase as work begins. Once a
  cluster closes, strike its row from the table here (or archive
  this doc when all rows resolve).
- The CI session that produced this catalog also landed the
  `NROS_PLATFORM_ALIASES` vendor-side wiring (Phases 154/159) and
  the `NROS_ZENOH_PLATFORM_USES_UNIX` POSIX+NuttX gate, which
  dropped the fail count from 189 → 63. Further large drops are
  unlikely without per-cluster investigation.
- `test_qemu_rtic_*` (cluster F) was the only NEW regression
  surfaced by this session's churn — pre-Phase 156 baseline had
  these passing. Triage in F is the highest-priority next step.
