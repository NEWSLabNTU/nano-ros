# Phase 199 — `test-all` runtime-gap triage

**Goal.** Track and close the residual `just test` failures that remain after a
clean `nros setup` + `build-all` on a fully-provisioned host. These are
*runtime / feature* gaps, not setup or fixture-staging gaps — the latter were
all resolved during the 2026-05-29 sweep (see "Already fixed" below).

**Status.** Proposed (2026-05-29). Captured from a full local sweep on `main`
(`663 tests run: 643 passed, 20 failed, 110 skipped`). No new runtime fixes
yet — this doc inventories the 20 remaining failures so they route to the right
owning phase instead of being re-rediscovered each sweep.

**Priority.** P2 — none block setup/build; each is a real but bounded runtime
feature gap or an opt-in SDK shell. The zephyr CycloneDDS cluster (199.1) is the
largest and most product-relevant.

**Depends on.** Phase 177.2 (zephyr CycloneDDS actions + cross-impl), Phase
196.1 (zpico-link / zephyr fixture link), Phase 197 (`just setup` → canonical
`nros setup`). This phase does not duplicate those; it points at them.

---

## Overview

After the sweep, every remaining failure runs against a *built, booting*
fixture (or a deliberately opt-in SDK shell) — i.e. the binary exists and the
test executes; it fails on runtime behaviour or a known build-wiring gap, not
on a missing tool/fixture. The 20 cluster into five groups.

## Already fixed (this sweep — do **not** re-file)

- **Tool discovery** — `nros-tests` resolvers (`xrce_agent`, `zenohd`, `qemu`)
  now fall back to the `nros setup` SDK store via `nros_store_bin(tool, exe)`.
- **Zephyr host idlc** — `zephyr/CMakeLists.txt` resolved the retired
  `build/install/bin/idlc`; now resolves explicit env → nros store → PATH →
  legacy in-tree. Recovered every zephyr c/cpp CycloneDDS fixture *build*.
- **Zephyr fixture path** — `fixtures::binaries::zephyr_build_root()` now
  mirrors the build's `ZEPHYR_WORKSPACE` selection (in-tree → legacy
  `../nano-ros-workspace` sibling → build-tree), so c/cpp CycloneDDS e2e stopped
  false-skipping and now actually run.
- **zenoh-posix archive fixture** — `just build-zenoh-posix-fixture` staged;
  fixes `zenoh_archive_symbols`, `zenoh_header_parity`, `zpico_build_matrix`.

---

## Work Items

### 199.1 — Zephyr CycloneDDS runtime e2e (→ Phase 177.2 / 196.1) — 11

Fixtures build + boot on `native_sim`, but the CycloneDDS data plane does not
complete. Two sub-causes:

- **Data plane / discovery (runtime).** `zephyr_{c,cpp}_cyclonedds_pubsub_e2e`
  exchange no samples; `zephyr_{c,cpp}_cyclonedds_service_e2e` get no `[OK]`
  reply. Suspect embedded Cyclone discovery / multicast under native_sim
  (`<AllowMulticast>` + loopback). `zephyr_dds_{c,cpp,rs}_action_e2e` —
  actions on zephyr CycloneDDS not implemented (Phase 177.2 explicitly defers).
- **rust+cyclonedds link gap (build).** `zephyr_rust_cyclonedds_{pubsub,service}_e2e`
  and `zephyr_rust_{talker,listener}_cyclonedds_boot` fail to *build*: the Rust
  app pulls `nros_rmw_zenoh::zpico::*` and link-errors on `zpico_open` /
  `zpico_spin_once` / … with no zpico staticlib in a CycloneDDS build. Zephyr
  rust **zenoh** builds clean — only the cyclonedds combo is unwired (Phase
  196.1 / 175 follow-up).

**Files.** `packages/testing/nros-tests/tests/phase_118_collapse.rs`,
`packages/testing/nros-tests/tests/zephyr.rs`,
`packages/dds/nros-rmw-cyclonedds/`, `zephyr/CMakeLists.txt` (rust+cyclone
link), `examples/zephyr/rust/*`.

### 199.2 — XRCE action/service runtime e2e — 4

`{c_,}xrce` `action_fibonacci` + `service_request_response`: the agent is found
and both binaries spawn, but the goal is never accepted / the reply never lands
(`Goal accepted` / `[OK]` absent). Runtime over the MicroXRCEAgent, not setup.
Triage: agent transport vs. nros XRCE action/service completion.

**Files.** `packages/testing/nros-tests/tests/c_xrce_api.rs`,
`packages/testing/nros-tests/tests/xrce.rs`,
`examples/native/c/{action-server,action-client,service-server,service-client}/`.

### 199.3 — NuttX runtime e2e — 2

`rtos_e2e::…Nuttx…Lang__C` service e2e and
`nuttx_make_e2e::nuttx_external_apps_link_into_kernel_binary`. Cross-ref Phase
194 (nuttx provisioning) — confirm whether these are provisioning-residual or
genuine runtime.

**Files.** `packages/testing/nros-tests/tests/rtos_e2e.rs`,
`packages/testing/nros-tests/tests/nuttx_make_e2e.rs`.

### 199.4 — ESP32 logging smoke — 1

`logging_smoke_esp32_qemu_emits_every_severity` — fixture boots under QEMU but
does not emit every severity line. Triage logging backend vs. esp32-qemu boot.

**Files.** `packages/testing/nros-tests/tests/logging_smoke.rs`.

### 199.5 — External opt-in SDK shells — 3 (expected-skip candidates)

`integration_{esp_idf,platformio,zephyr}_integration_shell_smoke` need the
`extended`-tier SDKs (ESP-IDF, PlatformIO) that `just setup` (default tier) does
not install. **Decision needed:** these should `skip!` with a precondition
message when the SDK is absent (they currently hard-fail), per the
"check existence + warn, don't build" testing principle — vs. keeping them as
fail-loud and excluded from the default `just test` filterset.

**Files.** `packages/testing/nros-tests/tests/integration_{esp_idf,platformio,zephyr}.rs`.

---

## Acceptance

- [ ] 199.1 zephyr CycloneDDS c/cpp pubsub+service exchange data on native_sim
- [ ] 199.1 rust+cyclonedds zephyr links (zpico provider wired or backend-gated)
- [ ] 199.1 zephyr CycloneDDS actions implemented (or explicitly skip! pending 177.2)
- [ ] 199.2 XRCE action/service e2e complete goal→result over the agent
- [ ] 199.3 nuttx C service e2e + external-apps link pass
- [ ] 199.4 esp32 logging smoke emits every severity
- [ ] 199.5 opt-in SDK shells gated as precondition-skip when SDK absent

## Notes

- Baseline sweep (2026-05-29, `main` @ post-codegen-retirement, nros 0.3.0):
  `663 run, 643 passed, 20 failed, 110 skipped`. Failure list is the union of
  199.1–199.5.
- The progression across the sweep was 53 → 39 → 24 → 20 failures as the
  setup/fixture-staging fixes landed; the floor of 20 is the runtime/external
  set tracked here.
- 199.5 is the only item with a *testing-policy* decision (skip vs. fail-loud);
  the rest are feature/runtime work owned by 177.2 / 196.1 / 194.
