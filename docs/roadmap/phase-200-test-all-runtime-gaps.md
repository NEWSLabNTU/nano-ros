# Phase 200 — `test-all` runtime-gap triage

**Goal.** Track and close the residual `just test` failures that remain after a
clean `nros setup` + `build-all` on a fully-provisioned host. These are
*runtime / feature* gaps, not setup or fixture-staging gaps — the latter were
all resolved during the 2026-05-29 sweep (see "Already fixed" below).

**Status.** Proposed (2026-05-29). Captured from a full local sweep on `main`
(`663 tests run: 643 passed, 20 failed, 110 skipped`). No new runtime fixes
yet — this doc inventories the 20 remaining failures so they route to the right
owning phase instead of being re-rediscovered each sweep.

**Priority.** P2 — none block setup/build; each is a real but bounded runtime
feature gap or an opt-in SDK shell. The zephyr CycloneDDS cluster (200.1) is the
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

### 200.1 — Zephyr CycloneDDS runtime e2e (→ Phase 177.2 / 196.1) — 11

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

### 200.2 — XRCE action/service runtime e2e — mostly FIXED

**Root cause (fixed).** `xrce_service_{client,server}_create` (service.c) were
missing the `const nros_rmw_qos_t *qos` parameter that the
`nros_rmw_vtable_t` `create_service_{client,server}` typedef grew in the Phase
193.5 QoS work. The cffi caller passed 7 args (`…, domain_id, &qos, &out`); the
C impls declared 6, so the impl read `&qos` as its `out` and wrote
`backend_data` into the QoS struct — the real `out->backend_data` stayed null,
and the cffi wrapper returned `ServiceClientCreationFailed` *after* the
requester/replier had actually been created successfully on the agent. Adding
the `qos` param (and honoring it, falling back to services-default when null)
restored registration. **Recovered:** `c_xrce_action_fibonacci` (+ the XRCE
service/action *registration* path for C / C++ / Rust, all of which share
`service.c`).

**Remaining (1) — service request/reply data plane.**
`test_c_xrce_service_request_response`: with registration fixed, the client now
reaches "Calling service…" and the server reaches "Waiting for service
requests", but the client's requests never reach the replier (`Total requests
handled: 0`). The *action* roundtrip (which uses requesters internally) passes,
so the requester↔replier matching can work — triage why the plain AddTwoInts
requester→replier routing does not (suspect type/topic naming:
`AddTwoInts_Reply_` vs the DDS `AddTwoInts_Response_`, or replier-match timing).
Re-run the Rust/C++ `xrce` service tests after their fixtures rebuild against
the registration fix to confirm scope.

**Files.** `packages/xrce/nros-rmw-xrce/src/service.c` (fixed),
`packages/xrce/nros-rmw-xrce/src/internal.h` (fixed),
`packages/testing/nros-tests/tests/{c_xrce_api,xrce}.rs`.

**Files.** `packages/testing/nros-tests/tests/c_xrce_api.rs`,
`packages/testing/nros-tests/tests/xrce.rs`,
`examples/native/c/{action-server,action-client,service-server,service-client}/`.

### 200.3 — NuttX runtime e2e — 2

`rtos_e2e::…Nuttx…Lang__C` service e2e and
`nuttx_make_e2e::nuttx_external_apps_link_into_kernel_binary`. Cross-ref Phase
194 (nuttx provisioning) — confirm whether these are provisioning-residual or
genuine runtime.

**Files.** `packages/testing/nros-tests/tests/rtos_e2e.rs`,
`packages/testing/nros-tests/tests/nuttx_make_e2e.rs`.

### 200.4 — ESP32 logging smoke — 1

`logging_smoke_esp32_qemu_emits_every_severity` — fixture boots under QEMU but
does not emit every severity line. Triage logging backend vs. esp32-qemu boot.

**Files.** `packages/testing/nros-tests/tests/logging_smoke.rs`.

### 200.5 — External opt-in SDK shells — 3 (expected-skip candidates)

`integration_{esp_idf,platformio,zephyr}_integration_shell_smoke` need the
`extended`-tier SDKs (ESP-IDF, PlatformIO) that `just setup` (default tier) does
not install. **Decision needed:** these should `skip!` with a precondition
message when the SDK is absent (they currently hard-fail), per the
"check existence + warn, don't build" testing principle — vs. keeping them as
fail-loud and excluded from the default `just test` filterset.

**Files.** `packages/testing/nros-tests/tests/integration_{esp_idf,platformio,zephyr}.rs`.

---

## Acceptance

- [ ] 200.1 zephyr CycloneDDS c/cpp pubsub+service exchange data on native_sim
- [ ] 200.1 rust+cyclonedds zephyr links (zpico provider wired or backend-gated)
- [ ] 200.1 zephyr CycloneDDS actions implemented (or explicitly skip! pending 177.2)
- [ ] 200.2 XRCE action/service e2e complete goal→result over the agent
- [ ] 200.3 nuttx C service e2e + external-apps link pass
- [ ] 200.4 esp32 logging smoke emits every severity
- [ ] 200.5 opt-in SDK shells gated as precondition-skip when SDK absent

## Notes

- Baseline sweep (2026-05-29, `main` @ post-codegen-retirement, nros 0.3.0):
  `663 run, 643 passed, 20 failed, 110 skipped`. Failure list is the union of
  200.1–200.5.
- The progression across the sweep was 53 → 39 → 24 → 20 failures as the
  setup/fixture-staging fixes landed; the floor of 20 is the runtime/external
  set tracked here.
- 200.5 is the only item with a *testing-policy* decision (skip vs. fail-loud);
  the rest are feature/runtime work owned by 177.2 / 196.1 / 194.
