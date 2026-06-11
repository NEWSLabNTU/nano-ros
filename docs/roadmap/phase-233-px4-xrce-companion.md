# Phase 233 — PX4 companion path over XRCE-DDS

**Goal.** Stand up the *companion / peer* PX4 position: an embedded nano-ros node
that talks `px4_msgs` to the same Micro XRCE-DDS Agent PX4's `uxrce_dds_client`
uses — the **mainstream PX4↔ROS 2 integration**, which nano-ros's `nros-rmw-xrce`
backend already fits but does not yet exercise. This is **Track B** of the
two-track PX4 plan in **RFC-0039** ("support both") — the *additive* track.

**Status.** Companion path complete + validated against real PX4 SITL (2026-06).
233.1–233.5 landed; the topic path interoperates with actual PX4 firmware.
**233.6 (service/action XRCE-DDS interop) is open** — the CDR-header fix covers
topics, not yet services/actions (ROS 2 service interop, off the PX4 critical
path). Wiring the harness also surfaced + fixed a spin-pacing bug
([issue 0026](../issues/archived/0026-px4-xrce-bare-agent-type-matching.md),
resolved). Design-of-record: RFC-0039 (Draft).

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
  the agent) **+ `test_px4_companion_cross_session_receive`** (companion subscribes
  `/fmu/out/*` while the stub streams it, asserts the companion receives ≥5 samples).
  Both **pass** over a real agent.
- **Bug found + fixed.** Wiring 233.4 surfaced an `nros-rmw-xrce` spin-pacing bug:
  XRCE is a poll-based backend, so `spin_once(t)` paces by relying on the backend to
  block for `t` — but `uxr_run_session_time` returned in ~0 µs when the session held a
  publisher (unconfirmed output), so a pub+sub node free-ran its spin loop and sent
  `DELETE_CLIENT` before DDS discovery, never receiving. `xrce_session_drive_io` now
  paces to its timeout. (Two earlier wrong theories — "type matching" and
  "mixed-direction reader+writer" — are recorded in
  [issue 0026](../issues/archived/0026-px4-xrce-bare-agent-type-matching.md), resolved.)

### 233.5 — Real PX4 SITL end-to-end + interop fixes  ✅
Validate Track B against **actual PX4 firmware**, not the stub.
- **Landed.** `examples/px4/rust/xrce/px4-probe` (subscribes `/fmu/out/timesync_status`,
  flows headless) + `nros-px4-sitl-test::px4_xrce_e2e` (`just px4 test-sitl`): boots real
  PX4 SITL, points its `uxrce_dds_client` at a `MicroXRCEAgent`, asserts the nano-ros
  probe receives real PX4 telemetry over `nros-rmw-xrce`. **Passes.**
- **Two interop bugs found + fixed** (the agent delivered real samples but nano-ros
  dropped them — caught with tshark + a logging agent + the probe's raw hexdump):
  1. **CDR encapsulation header.** nano-ros wrapped the XRCE DATA payload in a 4-byte CDR
     header; PX4 / real ROS 2 send the bare sample (the DDS representation header is the
     agent's concern). The 4-byte misalignment dropped every inbound PX4 sample.
     `publisher.c` strips the header before `uxr_buffer_topic`; `subscriber.c` re-prepends
     it on receive — symmetric, so nano-ros↔nano-ros is unchanged and nano-ros↔PX4 works.
  2. **`px4()` QoS durability.** Was `TRANSIENT_LOCAL`; PX4's `/fmu/out` writers are
     `VOLATILE`, so a transient-local reader never matched. Now `BEST_EFFORT + VOLATILE +
     KEEP_LAST(1)`.
  3. **Large-message (`publish_streamed`) header.** The streamed publish path wrote the
     serialized message (header included) straight into the zero-copy stream region, so
     large topics still shipped the 4-byte header. Now stages the message and copies the
     header-stripped body into the reserved slot. Validated by
     `nros-tests::xrce::test_xrce_large_message_publish` (passes).

### 233.6 — Service / action XRCE-DDS interop  ⬜ (planned)
The CDR-header strip/prepend (233.5.1) covers **topics** (`publisher.c` /
`subscriber.c`). The **service** (`service.c` — `uxr_buffer_request` /
`uxr_buffer_reply` + the request/reply inbox `memcpy`s) and **action** paths still
carry the executor's 4-byte CDR encapsulation header on the XRCE wire. This is
self-consistent (nano-ros↔nano-ros services/actions pass:
`test_xrce_service_request_response`, `test_xrce_action_fibonacci`) but will **not
interop with real ROS 2 service/action endpoints** over the agent, the same way
topics didn't before 233.5.1. PX4's companion path is topic-only, so this is not on
the PX4 critical path — but ROS 2 service/action interop over XRCE needs it.
- **Work items:**
  - `service.c` — strip the header before `uxr_buffer_request` / `uxr_buffer_reply`
    (requester + replier), and re-prepend it on the request/reply inbox writes (the two
    `memcpy(slot->data, ub->iterator, len)` sites), mirroring the topic fix. Mind the
    request/reply `SampleIdentity` framing — confirm where the header sits relative to it.
  - Actions ride on services + topics (feedback/status), so they inherit the service fix
    plus the already-landed topic fix; verify the goal/result/feedback/status round-trip
    end-to-end.
  - Add a ROS 2 interop test (real `rmw_fastrtps` service client/server via the agent,
    mirroring `xrce_ros2_interop.rs`) as the acceptance — the nano-ros↔nano-ros tests
    can't catch this class (both sides consistently wrong).
- **Acceptance:** a nano-ros XRCE service server answers a real ROS 2 service client (and
  vice-versa) over a `MicroXRCEAgent`.

## Acceptance

- nano-ros generates CDR `px4_msgs` from the shared PX4 `.msg` tree (no ament dep).
- A "PX4" QoS profile (`BEST_EFFORT`/`VOLATILE`/`KEEP_LAST`) is selectable and matches
  real PX4 `/fmu/out` writers.
- The companion example subscribes `/fmu/out/*` and publishes `/fmu/in/*` against
  `MicroXRCEAgent` and **receives** — regression-guarded by
  `nros-tests::px4_xrce::test_px4_companion_cross_session_receive`, and validated against
  **real PX4 SITL** by `nros-px4-sitl-test::px4_xrce_e2e`.
- `examples/README.md` matrix updated for the px4 XRCE cell.

## Notes

- All in this tree (codegen + xrce backend + example) — no px4-rs dependency, unlike
  Track A. The two tracks are independent.
- The `version` field travels in the payload; the agent translation node handles
  cross-version matching — so this path tolerates a px4_msgs version different from the
  firmware build (RFC-0039 OQ2/decision).
- Cross-ref RFC-0039 revision opportunity #5.
