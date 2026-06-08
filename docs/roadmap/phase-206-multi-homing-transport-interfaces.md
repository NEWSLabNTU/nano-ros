# Phase 206 — Multi-homing: `[[transport]].interfaces` wire emission

**Goal.** Make a node's transport span several NICs as **one merged graph** — the
common "node reachable on multiple interfaces" need that stock DDS/zenoh do
natively. Turn the already-plumbed `[[transport]].interfaces` list into real
per-backend NIC binding.

**Status.** Proposed (2026-05-29). Extracted from Phase 172.K.7 (archived) — its
schema + plumbing half landed; this phase is the deferred **wire-emission** half.

**Priority.** P2 — no shipped capability depends on it; meaningful only once a
multi-NIC target exists. Cyclone is the one backend where it's both meaningful
*and* testable today.

**Depends on.** Phase 172.K.7 schema/plumbing (landed); Phase 175.A (native
Cyclone build path) for the Cyclone config seam (206.3).

## Overview

`[[transport]].interfaces = ["eth0", "eth1"]` already parses → `PlanTransport.interfaces:
Vec<String>`, validates (ethernet/wifi only), and the generator emits a no-op
`BoardTransportConfig::set_interfaces(&[…])` board-Config call; both CMake parsers
accept the TOML array. **But nothing binds an actual NIC yet** — the seam is inert.

Three blockers gate real binding, in dependency order: a multi-endpoint runtime
`SessionSpec` (206.1), the per-backend mapping (206.2 zenoh decision, 206.3
Cyclone `<Interfaces>` emission), and a multi-NIC target to verify against
(206.4). Distinct from Phase 172.K.5 (multi-domain = *segregate* sessions); this
is *merge* — one session, many NICs.

Design: [`docs/design/0004-configuration-and-transports.md`](../design/0004-configuration-and-transports.md)
("Two axes" taxonomy, cases B/C).

## Landed (Phase 172.K.7 schema + plumbing, 2026-05-29)

- `PlanTransport.interfaces: Vec<String>` (serde default, skip-when-empty);
  `validate_transports` rejects it on serial/can (ethernet/wifi only).
- Generator emits `c.set_interfaces(&[…])` (mirrors `set_ssid`/`set_mac`), backed
  by a default-no-op `BoardTransportConfig::set_interfaces` seam.
- Both CMake parsers (`NanoRosConfig.cmake`, nros-c `NanoRosReadConfig.cmake`)
  accept `interfaces = ["eth0","eth1"]` (legacy scalar `interface` mirrored in) →
  `NROS_CONFIG_INTERFACES` list var.
- Tests: `transport_tests::{multi_homed_interfaces_parse_and_validate,
  interfaces_absent_round_trips_empty_and_skips_serialization,
  interfaces_are_ethernet_wifi_only}` + `multi_homed_interfaces_emit_set_interfaces_call`.

## Architecture

The merge lives at two layers: the **runtime** (`nros` — a `SessionSpec` that
carries N endpoints + `open_multi` wiring them onto one session) and the
**backend config** (the generator emitting each backend's native multi-NIC
directive: Cyclone `<General><Interfaces>`, zenoh `listen`/`connect` per NIC +
`scouting.multicast.interface`, Fast DDS whitelist). zenoh-pico clients are a
special case — a single locator to the router, so node-level multi-listen is
largely the router's concern and needs a semantics decision before emission.

## Work Items

### 206.1 — Multi-endpoint `SessionSpec` (runtime, `nros`)
- [ ] `SessionSpec` carries a **list** of endpoints (locator + per-endpoint
      interface), not one `locator`; `RmwConfig` threads the list to the backend;
      `Executor::open_multi` wires N endpoints onto one session as a single graph
      (distinct from K.5's one-session-per-spec multi-domain path).
- [ ] Backwards-compatible: the single-locator `SessionSpec::new(rmw, locator)`
      stays (one-endpoint list); existing callers unchanged.
- [ ] **Files:** `packages/core/nros-node/src/executor/spin.rs` (`SessionSpec`,
      `open_multi`), `packages/core/nros-rmw/src/` (`RmwConfig`).
- [ ] **Acceptance:** a `nros-node` unit test opens a session bound to ≥2
      endpoints and asserts each is wired (mock backend records the endpoint list).
      The prerequisite for any real merge.

### 206.2 — zenoh-pico multi-homing semantics (decision + emission)
- [ ] Decide what `interfaces` means for a zenoh-**pico client** (single locator
      to the router): (a) no-op at node level with a documented rationale (the
      router multi-homes), or (b) map to multiple `connect` endpoints / a scouting
      interface hint. Capture the decision in the design doc.
- [ ] Emit accordingly (or explicit, documented no-op) from the generator's
      `set_interfaces` seam for zenoh boards.
- [ ] **Files:** `docs/design/0004-configuration-and-transports.md`,
      generator `set_interfaces` emission, `nros-rmw-zenoh`.
- [ ] **Acceptance:** the decision is documented and the generator's zenoh output
      matches it (a generate-test asserting the emitted call/no-op).

### 206.3 — Cyclone `<Interfaces>` config emission (generator → CycloneDDS)
- [ ] Build the generator → Cyclone-config seam: emit
      `<General><Interfaces><NetworkInterface name="…"/></Interfaces>` from
      `interfaces`, fed to Cyclone via a generated `CYCLONEDDS_URI` fragment (or a
      generated config file the native Cyclone build consumes). The generator emits
      **no** Cyclone config today (it lives in `session.cpp`'s
      `kEmbeddedCycloneConfig` / `CYCLONEDDS_URI` env) — this is a new path.
- [ ] Wire `BoardTransportConfig::set_interfaces` (or the native equivalent) so it
      actually constrains Cyclone's NIC binding, not a no-op.
- [ ] **Files:** generator Cyclone-config emission, `packages/dds/nros-rmw-cyclonedds/`
      (`session.cpp`), the native Cyclone build path (Phase 175.A).
- [ ] **Acceptance:** the generated Cyclone config (URI/file) contains the declared
      NICs; a generate-test asserts the `<Interfaces>` emission. **Depends on 206.1.**

### 206.4 — Multi-NIC verification target (hosted Cyclone)
- [ ] A hosted Cyclone build/test that binds **specific** NICs and verifies the
      binding takes effect — every board has one NIC, but a host has `lo` + a real
      NIC, so `interfaces = ["lo"]` (or `["lo","<eth>"]`) is the first
      meaningful + testable case.
- [ ] Verify the merge end-to-end: communication is constrained to / spans the
      declared interfaces (e.g. loopback-only when `["lo"]`, or Cyclone's bound-
      interface log/introspection confirms the set).
- [ ] **Files:** `packages/testing/nros-tests/` (or the codegen `orchestration_e2e`),
      a hosted Cyclone fixture.
- [ ] **Acceptance:** an e2e proving the declared `interfaces` actually bind.
      **Depends on 206.3.** This is the gate that makes 206 worth finishing.

### 206.5 — Fast DDS whitelist (future, out of scope)
- [ ] When/if a Fast DDS backend lands, map `interfaces` → its interface
      whitelist. Tracked here for completeness; not actionable until that backend
      exists.

## Acceptance

- [ ] A node with `[[transport]].interfaces = ["a","b"]` binds **both** NICs as
      one merged graph on at least one backend (Cyclone), verified by a runtime
      test on a multi-NIC (host `lo` + NIC) target.
- [ ] `SessionSpec` carries multiple endpoints; `open_multi` wires them (unit test).
- [ ] The zenoh-pico semantics are decided + documented (no silent no-op).
- [ ] The Cyclone `<Interfaces>` config is generator-emitted (not hand-written),
      with a generate-test fixture.

## Notes

- **Merge vs segregate.** Phase 172.K.5 (multi-domain) opens one session *per*
  domain (segregate); this phase merges N NICs into *one* session. Both use
  `open_multi`, but 206.1's endpoint-list spec is the new primitive.
- **Why Cyclone first.** It's the only backend where multi-homing is both
  meaningful (real `<Interfaces>` directive) and testable today (a host has ≥2
  NICs). zenoh-pico's single-locator client model makes node-level multi-listen
  the router's concern (206.2 decides this); Fast DDS doesn't exist yet (206.5).
