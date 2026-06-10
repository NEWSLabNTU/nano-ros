# Phase 233 — PX4 companion path over XRCE-DDS

**Goal.** Stand up the *companion / peer* PX4 position: an embedded nano-ros node
that talks `px4_msgs` to the same Micro XRCE-DDS Agent PX4's `uxrce_dds_client`
uses — the **mainstream PX4↔ROS 2 integration**, which nano-ros's `nros-rmw-xrce`
backend already fits but does not yet exercise. This is **Track B** of the
two-track PX4 plan in **RFC-0039** ("support both") — the *additive* track.

**Status.** Complete (2026-06). All four work items landed (233.1 codegen, 233.2 QoS,
233.3 example, 233.4 doc + CI round-trip). The CI round-trip is single-session; the
cross-session companion ↔ PX4 receive needs a typed agent (PX4 SITL or `-r refs`),
tracked as a follow-up in [issue 0026](../issues/0026-px4-xrce-bare-agent-type-matching.md).
Design-of-record: RFC-0039 (Draft).

**Priority.** P2 — additive (nothing breaks without it), but it is the path most
PX4+ROS 2 users actually deploy, and nano-ros is already 90% wired for it.

**Depends on.** RFC-0039 (umbrella + decision), `nros-rmw-xrce` (XRCE-DDS client;
already supports `TRANSIENT_LOCAL`, `session.c`), `rosidl-codegen` (CDR message
emit), the PX4 `.msg` tree (shared with Phase 232's pin). RFC-0031 (RMW selection).

## Overview

PX4's mainstream ROS 2 integration is the uXRCE-DDS bridge: the firmware
`uxrce_dds_client` connects to `MicroXRCEAgent` on a companion over serial/UDP; the
agent exposes uORB topics as ROS 2 `px4_msgs` topics (`/fmu/out/*` = PX4→ROS,
`/fmu/in/*` = ROS→PX4). `nros-rmw-xrce` is itself an XRCE-DDS client, so a nano-ros
node on a peer MCU can connect to the *same agent* and exchange `px4_msgs` — an
embedded ROS 2 node alongside PX4. v1.16's translation node buffers version skew, so
this path is version-tolerant (unlike Track A's uORB).

Two pieces are missing: a `px4_msgs` **CDR** emitter (today's PX4 codegen emits raw
`repr(C)` for uORB, not CDR for XRCE) and an **example** + QoS match.

## Architecture

```
PX4 .msg tree (shared pin) ─► rosidl-codegen (CDR) ─► px4_msgs::msg::* nano-ros types
                                                              │
nano-ros peer (nros-rmw-xrce) ──XRCE serial/UDP──► MicroXRCEAgent ──DDS──► PX4 + ROS 2
  QoS: TRANSIENT_LOCAL + BEST_EFFORT + KEEP_LAST          (translation node buffers
  topics: /fmu/in/*, /fmu/out/*                            message versions)
```

## Work Items

### 233.1 — `px4_msgs` CDR emit (one-source, two-emitters)  ✅
Per RFC-0039 OQ2: feed the *same* PX4 `.msg` tree (`msg/` + `msg/versioned/`) into
`rosidl-codegen` to emit CDR-serializable `px4_msgs::msg::*` types for the XRCE path —
no external ament `px4_msgs` dependency. Settle: (a) `rosidl-codegen` accepting the
PX4 `MESSAGE_VERSION` constant; (b) the `version` field is a normal payload field —
generate it; (c) type *names* must be `px4_msgs::msg::*` (DDS matches by topic+type
name; nano-ros `type_hash` is orthogonal to PX4's `message_hash`).
- **Files:** `rosidl-codegen` (PX4-`.msg` acceptance), a PX4 px4_msgs generation
  entry (CLI/CMake), shared with the Phase 232 pin.
- **Acceptance:** `VehicleOdometry`/`OffboardControlMode`/`VehicleCommand` generate as
  CDR types that round-trip.
- **Landed.** `nros generate-px4-msgs --px4 <tree> --output <dir>` stages `msg/` +
  `msg/versioned/` into one flat `px4_msgs` package and reuses `generate_package`
  (`rosidl-bindgen::generator::generate_px4_msgs`). 235 messages emit; generated crate
  `cargo check`s clean against local `nros-core`/`nros-serdes`; the three acceptance
  types round-trip (serialize → deserialize → eq, hermetic test under `tmp/`). Type
  names are `px4_msgs::msg::dds_::*_`; `MESSAGE_VERSION` and the `version` field emit
  as normal payload.

### 233.2 — QoS profile matching PX4  ✅
PX4 publishers are `TRANSIENT_LOCAL + BEST_EFFORT + KEEP_LAST`. `nros-rmw-xrce`
supports `TRANSIENT_LOCAL` (`session.c:177`); confirm `BEST_EFFORT` reliability is
exposed and add a "PX4" QoS profile so a nano-ros sub on `/fmu/out/*` matches (default
reliable+volatile won't connect).
- **Files:** `packages/xrce/nros-rmw-xrce` QoS mapping; a named profile surfaced in
  the user API.

### 233.3 — Companion example  ✅
Add `examples/px4/<lang>/xrce/<example>/` — a nano-ros node that subscribes
`/fmu/out/vehicle_odometry` and publishes `/fmu/in/offboard_control_mode` (or
`vehicle_command`) against a running `MicroXRCEAgent`, with the PX4 QoS profile. The
first companion-side PX4 example (existing px4 examples are uORB in-firmware only).
- **Files:** `examples/px4/.../xrce/...`, `examples/README.md` coverage matrix.
- **Landed.** `examples/px4/rust/xrce/offboard-companion/` — subscribes
  `/fmu/out/vehicle_odometry` + streams `/fmu/in/offboard_control_mode` at ~10 Hz with
  `QosSettings::px4()`. Standalone copy-out carrying a trimmed pre-generated
  `generated/px4_msgs/` (VehicleOdometry + OffboardControlMode). Builds + links the XRCE
  backend; connects to a live agent and creates entities/streams setpoints (verified
  locally). README matrix px4-rust xrce cell updated.

### 233.4 — Agent bring-up doc + test harness  ✅
Document standing up `MicroXRCEAgent` (udp4 `-p 8888` / serial) for the example, and
wire a host test (SITL or a stubbed agent) so CI can exercise the round-trip. Mirror
the existing zenohd/cyclonedds support-service pattern (not a platform scope).
- **Files:** a docs/reference PX4-companion guide; `nros_tests` agent helper.
- **Landed.** `docs/reference/px4-xrce-companion.md` (agent + SITL + QoS bring-up);
  `examples/px4/rust/xrce/px4-stub/` (fake-PX4 publisher with a `PX4_STUB_LOOPBACK`
  self-test mode); `just px4 build-fixtures` (generate px4_msgs + build the stub to
  `target-xrce/`); `nros-tests::px4_xrce::test_px4_msgs_roundtrip_over_agent` (starts
  `XrceAgent`, runs the stub loopback, asserts ≥5 `VehicleOdometry` round-trip through
  the agent). **Passes** — the full `px4_msgs` CDR + `px4()` QoS + XRCE pub/sub path is
  CI-covered over a real agent.
- **Scope note.** The CI round-trip is single-session (loopback). The *cross-session*
  companion ↔ PX4 receive needs a typed agent (PX4 SITL or `-r refs`) — a bare agent
  matches non-built-in types only intra-participant. Refined analysis +
  remaining-work tracking: [issue 0026](../issues/0026-px4-xrce-bare-agent-type-matching.md).

## Acceptance

- nano-ros generates CDR `px4_msgs` from the shared PX4 `.msg` tree (no ament dep).
- A "PX4" QoS profile (`TRANSIENT_LOCAL`/`BEST_EFFORT`/`KEEP_LAST`) is selectable.
- The companion example subscribes `/fmu/out/*` and publishes `/fmu/in/*` against
  `MicroXRCEAgent`. Publish/connect verified against a bare agent; full receive
  round-trip needs a typed agent (PX4 SITL or `-r refs`) — [issue 0026](../issues/0026-px4-xrce-bare-agent-type-matching.md).
- `examples/README.md` matrix updated for the px4 XRCE cell.

## Notes

- All in this tree (codegen + xrce backend + example) — no px4-rs dependency, unlike
  Track A. The two tracks are independent.
- The `version` field travels in the payload; the agent translation node handles
  cross-version matching — so this path tolerates a px4_msgs version different from the
  firmware build (RFC-0039 OQ2/decision).
- Cross-ref RFC-0039 revision opportunity #5.
