# Phase 206 — Multi-homing: `[[transport]].interfaces` wire emission

**Goal.** Make a node's transport span several NICs as **one merged graph** — the
common "node reachable on multiple interfaces" need that stock DDS/zenoh do
natively. Turn the already-plumbed `[[transport]].interfaces` list into real
per-backend NIC binding.

**Status (2026-09-04). Proposed, and its foundation is GONE.** Still unstarted
as proposed 2026-05-29 — but the "Landed" section below is no longer true, and
the way it stopped being true is the useful part.

**172.K.7's half really did land. Three separate retirements took most of it
back, and none of them knew it.**

| landed 2026-05-29 | removed by | when |
| --- | --- | --- |
| generator emits `c.set_interfaces(&[…])`, + the test `multi_homed_interfaces_emit_set_interfaces_call` | `11a00b0f8` — #202, "retire the dead standalone-package pipeline" (it deleted `orchestration/generate.rs` whole) | 2026-07-16 |
| `NanoRosConfig.cmake` + `NanoRosReadConfig.cmake` parsing `interfaces` → `NROS_CONFIG_INTERFACES` | `f473b78b4` / `53f2ddce7` — phase-212 M-F.10, "retire cmake codegen of `NROS_APP_CONFIG`" | phase-212 |
| the `config.toml` readers those parsers lived in | phase-256 W9, the legacy `config.toml` reader removal | — |

Each retirement had a good local reason and each was right on its own terms.
Together they dismantled this phase one plank at a time, and nobody updating
those commits was looking at this doc.

**What actually survives, verified on `main` 2026-09-04:**

* `PlanTransport.interfaces: Vec<String>` — `orchestration/plan.rs:684`
* its ethernet/wifi-only validation — `plan.rs:854-856`
* its copy into the IR — `model_ingest.rs:1457`
* three of the four tests — `plan.rs:1022, 1036, 1052`, all **parse/validate
  only**; none tests emission, because there is no emission
* `BoardTransportConfig::set_interfaces` — `nros-platform/src/board/config.rs:108`,
  a default no-op with **zero overrides and zero call sites tree-wide**

So the value parses, validates, reaches the IR, is written into
`<bake>/nros-plan.json` — **and is read by nothing.** `nros explain` cannot even
print it (`explain.rs:384-396` prints `ip`/`device`/`baudrate` only).

**This phase is now an instance of the class it would have to fix.** phase-349
names the shape while flagging its sibling: `NROS_NETSTACK` is *"emitted too …
and nothing reads it — the same declared-but-unread shape."* `set_interfaces` is
the second live instance. Any revival that adds another declaration without a
consumer repeats the defect.

*Original status, 2026-05-29:* Proposed. Extracted from Phase 172.K.7 (archived)
— its schema + plumbing half landed; this phase is the deferred
**wire-emission** half.

**Priority.** P2 — no shipped capability depends on it; meaningful only once a
multi-NIC target exists. Cyclone is the one backend where it's both meaningful
*and* testable today.

**Depends on.** Phase 172.K.7 schema/plumbing (landed); Phase 175.A (native
Cyclone build path) for the Cyclone config seam (206.3).

## Overview

`[[transport]].interfaces = ["eth0", "eth1"]` already parses → `PlanTransport.interfaces:
Vec<String>` and validates (ethernet/wifi only). **The rest of this paragraph
was true in May and is not now** — there is no generator emitting
`set_interfaces` and there are no CMake parsers; see the status table above.
**Nothing binds an actual NIC**, and the seam is not merely inert, it is
unreachable: the only surviving consumer of the parsed value is the serializer
that writes it into `<bake>/nros-plan.json`.

Three blockers gate real binding, in dependency order: a multi-endpoint runtime
`SessionSpec` (206.1), the per-backend mapping (206.2 zenoh decision, 206.3
Cyclone `<Interfaces>` emission), and a multi-NIC target to verify against
(206.4). Distinct from Phase 172.K.5 (multi-domain = *segregate* sessions); this
is *merge* — one session, many NICs.

Design: [`docs/design/0004-configuration-and-transports.md`](../design/0004-configuration-and-transports.md)
("Two axes" taxonomy, cases B/C).

## Landed (Phase 172.K.7 schema + plumbing, 2026-05-29) — MOSTLY SINCE REMOVED

Read this list as the record of what K.7 built, not as the state of the tree.
The status table above says what took each piece back; **✓** survives, **✗**
does not.

- **✓** `PlanTransport.interfaces: Vec<String>` (serde default, skip-when-empty);
  `validate_transports` rejects it on serial/can (ethernet/wifi only).
  (`plan.rs:684`, `:854-856`)
- **✗** Generator emits `c.set_interfaces(&[…])` (mirrors `set_ssid`/`set_mac`),
  backed by a default-no-op `BoardTransportConfig::set_interfaces` seam.
  — the generator went with `orchestration/generate.rs` in `11a00b0f8`. The
  no-op trait method survives with no callers; `set_ssid` and `set_mac` lost
  their emitters in the same commit, so all three are dead together.
- **✗** Both CMake parsers (`NanoRosConfig.cmake`, nros-c `NanoRosReadConfig.cmake`)
  accept `interfaces = ["eth0","eth1"]` (legacy scalar `interface` mirrored in) →
  `NROS_CONFIG_INTERFACES` list var.
  — both files are gone; `NROS_CONFIG_INTERFACES` appears in no source or build
  output today, only in this doc and archived phase-172.
- **✓/✗** Tests: `transport_tests::{multi_homed_interfaces_parse_and_validate,
  interfaces_absent_round_trips_empty_and_skips_serialization,
  interfaces_are_ethernet_wifi_only}` **survive** (`plan.rs:1022, 1036, 1052`) —
  `multi_homed_interfaces_emit_set_interfaces_call` **does not**, deleted with
  `tests/orchestration_e2e.rs` in `11a00b0f8`.

> **Anyone reviving this phase should re-plan rather than resume.** 206.2 and
> 206.3 below name a "generator `set_interfaces` seam" as the thing to emit
> from, and that seam does not exist — an hour spent looking for it is the
> predictable cost of leaving this section unmarked. The surviving architecture
> reaches a backend by other routes (the RFC-0049 knob ladder already carries
> `NROS_BOARD_TOML` into `nros-zpico-build`), and the per-backend picture has
> changed too: zenoh-pico is structurally a client with one locator (peer mode
> is refused, `MULTICAST_TRANSPORT = false`), which bears directly on 206.2's
> open decision. None of that is decided here; this note only records that the
> plan below is written against a tree that no longer exists.

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
- [ ] **Files:** generator Cyclone-config emission, `packages/rmw/cyclonedds/nros-rmw-cyclonedds/`
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
