# Phase 266 — Declarative cross-RMW bridge: complete the bake→entry→build flow

Status: **In progress (2026-06-26)** — W0 done (planner transform + cyclone-Rust
codegen); W1–W5 remaining. · Implements [RFC-0009](../design/0009-bridge-topic-forwarding.md)
(bridge topic-forwarding) · Resolves [issue 0099](../issues/0099-declarative-bridge-planner-population.md)
· Completes [phase-263](phase-263-complete-workspace-examples.md) Track B / B3
(`ws-bridge-rust`).

> **Origin.** phase-263 B3 set out to ship `examples/workspaces/ws-bridge-rust` —
> a declarative cross-RMW gateway (`[[bridge]]` in `system.toml`) forwarding
> `/chatter` zenoh→cyclonedds in one process. A deep dive found the *config →
> plan → relay-codegen → runtime* pipeline is code-complete + unit-tested
> (RFC-0009), but a **cascade** of orchestration gaps sits between a declared
> `[[bridge]]` and a built, forwarding binary. The genuinely-blocking, reusable
> engine work landed (W0); the cascade is sized into the waves below. B3 is
> phase-sized, hence this doc.

## Why a separate phase

A declarative bridge is not "one more workspace example": it is the **first**
system that needs the *baked* orchestration entry on a pure-Rust workspace (every
other Track-B workspace uses the `nros::main!` macro, which emits no bridge
relay), and it exercises plan emitters + metadata collection + a second RMW
backend that the macro path never touches. Each gap below is independently real
and independently testable.

## W0 — Engine foundations. **DONE (2026-06-25).**

The reusable, RMW-agnostic core — benefits any bridge pair, not just cyclone.

- **Planner transform** (`4b5f6d8ff`, issue #99 step 0). `planner.rs` now emits,
  for a `[[bridge]]` system: `build.transports` (one `PlanTransport{rmw,domain,
  locator}` per distinct endpoint → `is_bridge()` true, `SESSION_SPECS` /
  `open_multi` light up) + `plan.bridges` (one `PlanBridge{name,connect,topics}`,
  endpoints byte-matching the transports so `bridge_endpoint_session_idx`
  resolves). Shared `resolve_bridge_endpoint` parses `"<rmw>:<domain>"` /
  bare-domain selectors; locator = `[system].locator` for the system-rmw endpoint,
  none for the DDS/multicast peer. 3 unit tests; validated end-to-end (`nros plan`
  on `ws-bridge-rust` → correct transports + bridges).
- **Cyclone as a first-class native Rust backend** (`bdc05d974`). The generated
  native Rust entry now links + registers `nros-rmw-cyclonedds-sys`
  (`render_one_backend` + `render_backend_register_fn`), gated on `board ∈
  {native, posix}` so non-native / CMake-corrosion builds stay byte-identical (no
  double-link; C/cpp workspaces don't use this generated-entry path). Test
  `cyclone_backend_dep_gated_on_native_board`. Found: `std_msgs/Int32` +
  `rmw_dds_common_graph` descriptors are baked into `nros-rmw-cyclonedds-sys/
  build.rs` by default, so the demo type needs no descriptor staging.
- **Workspace skeleton** (`088aea8e8`). `examples/workspaces/ws-bridge-rust`:
  `talker_pkg` (builds) + `demo_bringup/system.toml` (`[[domain]]`×2 + `[[bridge]]
  gw`). `nros plan` on it emits a correct bridge plan. README marks WIP.

## W1 — Carry the bridge plan through the bake emitter

**Gap:** `cmd/codegen_system.rs::render_plan_json` (the `nros codegen-system`
bake) is a SECOND, thin plan emitter — it writes its own `nros-system/
nros-plan.json` (a `PlanComponent` host-side record), NOT `planner::
schema_plan_json`. So the bake tree's plan has `bridged_rmws=null`,
`transports=null`, `bridges=null` even though `nros plan` populates them.

**Work:** decide the SSoT and converge — either route `codegen-system` through
`schema_plan_json` (preferred: one plan emitter), or apply the same transform in
`render_plan_json`. First nail down **which plan the baked entry build actually
consumes** (`build_generated_package`'s `plan_path`) — the bake's thin record, or
a planner-produced full plan — and make THAT one carry the bridge fields.

**Acceptance:** `nros codegen-system --bringup demo_bringup` on `ws-bridge-rust`
writes a plan whose `build.transports` + `bridges` are populated; a unit/golden
test on the bake output asserts it.

## W2 — Component metadata so forwarded topics resolve

**Gap:** `forwarded_topics` resolves the bridge's topic list from the plan's
`interfaces`, which come from per-component publisher/subscriber metadata. A
launch-only `nros plan` (or a metadata-less bake) leaves `interfaces=[]` →
`topics=[]` → the bridge forwards nothing.

**Work:** ensure the bridge build collects component metadata before planning (the
fixture/workspace lane builds the node pkgs → sidecar metadata; the standalone
path does not). Confirm `talker_pkg`'s `/chatter` `std_msgs/Int32` publisher
surfaces in `interfaces`, so `plan.bridges[0].topics == ["/chatter"]`.

**Acceptance:** the resolved `ws-bridge-rust` plan has `topics=["/chatter"]`;
`validate_bridges` passes (topic resolves to `std_msgs/Int32`).

## W3 — Pure-cargo baked Rust entry build lane

**Gap:** existing Rust workspaces build the `nros::main!` macro entry (no bridge
relay). The bridge needs the BAKED orchestration entry (`build_generated_package`
→ `generate_package` → `src/{lib,main}.rs` + `Cargo.toml` with the backend deps +
`register_bridges`), the path C/cpp drive via CMake. No lane builds a pure-cargo
*baked* Rust entry.

**Work:** wire the command sequence for a Rust workspace bridge — `nros ws sync`
→ `nros codegen-system` → generate the orchestration entry → `cargo build` — and
add a `[[workspace_fixture]]` lane (`examples/fixtures.toml`) for
`ws-bridge-rust`. The generated entry must link `nros-rmw-zenoh` +
`nros-rmw-cyclonedds-sys` (W0) and compile the vendored C++ CycloneDDS
(submodule + `cyclonedds-ci`-style gate).

**Acceptance:** `cargo build` of the generated `ws-bridge-rust` entry links clean
(both backends, `register_bridges` present); fixture lane builds it in CI
(gated-skip if the cyclonedds submodule is absent).

## W4 — Per-type cyclone descriptor staging in the generated relay

**Gap:** cyclone egress rejects a raw publisher whose type descriptor is not
registered. The generated `register_bridges` creates raw pubs by `(name, hash)`
only. Baked types (`std_msgs/Int32`, `rmw_dds_common_graph`) work; arbitrary
forwarded types do not.

**Work:** for cyclone endpoints, have the generated entry stage each forwarded
topic's descriptor before the publisher loop — wire `nros codegen
cyclonedds-descriptors` (already exists: `.msg` → IDL → `idlc` → `register.c` +
manifest) into the generated package's build, OR emit
`nros_rmw::register_type_descriptor(TYPE, <pkg>::msg::<Msg>::FIELDS)` (needs the
message crate as a generated-entry dep). Demo (`Int32`) is unblocked by W0's baked
default, so this wave can land after a green Int32 bridge.

**Acceptance:** a bridge forwarding a NON-baked custom type creates its cyclone
egress publisher without error.

## W5 — Runtime e2e (gated) + `ws-bridge-rust` completion

**Work:** boot zenohd + the baked `ws-bridge-rust` entry (talker + bridge) + a
stock `rmw_cyclonedds_cpp` subscriber; assert `ros2 topic echo /chatter` receives
the talker's counter — proving cross-RMW forward + ROS 2 interop. Honor #53
(egress domain threaded) + #67 (multi-RMW raw path). Flip the workspace README
from WIP to DONE; update the phase-263 B3 entry.

**Acceptance:** the runtime test passes where a live DDS peer is present (gated,
same contract as the existing `bridge-zenoh-to-cyclonedds-fwd` fixture); skips
cleanly otherwise.

## Sequencing

W1 → W2 → W3 unblock a *building* Int32 bridge (the visible milestone); W4 is
additive (non-baked types); W5 is the gated runtime proof. W1 is the immediate
blocker (the bake plan must carry the bridge before anything downstream sees it).

## Acceptance (phase)

- `examples/workspaces/ws-bridge-rust` builds via the documented bake flow, its
  generated entry linking both backends with the `register_bridges` relay.
- A gated runtime test proves zenoh→cyclonedds forwarding to a stock ROS 2 peer.
- Issue #99 resolved; phase-263 B3 flipped to DONE.
- The xrce variant (`zenoh↔xrce`) is reachable by the same flow (xrce is a wired
  Rust backend with lazy type registration — needs W1–W3, skips W4) — a
  lower-build-cost sibling, optional.
