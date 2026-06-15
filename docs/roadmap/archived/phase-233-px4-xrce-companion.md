# Phase 233 — PX4 companion path over XRCE-DDS

**Goal.** Stand up the *companion / peer* PX4 position: an embedded nano-ros node
that talks `px4_msgs` to the same Micro XRCE-DDS Agent PX4's `uxrce_dds_client`
uses — the **mainstream PX4↔ROS 2 integration**, which nano-ros's `nros-rmw-xrce`
backend already fits but does not yet exercise. This is **Track B** of the
two-track PX4 plan in **RFC-0039** ("support both") — the *additive* track.

**Status.** Companion path complete + validated against real PX4 SITL (2026-06).
233.1–233.5 landed; the topic path interoperates with actual PX4 firmware.
**233.6 (service/action XRCE-DDS interop): services + pub/sub + actions DONE both
directions (hard-asserted vs real `rmw_fastrtps` ROS 2)** — actions interop via
per-channel entity types + a fixed `uint8[16]` goal-id wire fix; deferred
`get_result` reply landed in [phase-237](phase-237-deferred-get-result.md) (off the
PX4 critical path). Wiring the harness also surfaced + fixed a spin-pacing bug
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

### 233.6 — Service / action XRCE-DDS interop  ✅ (services + pub/sub + actions, both directions; get_result deferral → phase-237)
**Wave 1 (DONE) — services + topics, forward.** The CDR-header strip/prepend now
covers `service.c` — the 5 sites below (3 outbound strip + 2 inbound prepend).
`test_xrce_service_ros2_client` is a **hard assert**: a real `rmw_fastrtps` ROS 2
service client gets `sum=8` from a nano-ros XRCE service server over a
`MicroXRCEAgent`. (Also fixed the interop tests' env: they set `XRCE_AGENT_ADDR`
but the examples read `NROS_LOCATOR`, so the nodes had been connecting to the
*default* agent — the whole interop suite was a no-op.) nano-ros↔nano-ros
services + actions stay symmetric (the `xrce` group still passes).

**Wave 2 (DONE) — reverse direction was test bugs, not nano-ros bugs.** The
reverse-direction "failures" were **all in the harness**, not the wire: the
CDR-header fix already made both directions interop. Three test bugs masked it:
- *`NROS_LOCATOR` not set* — the reverse nodes connected to the default agent,
  not the test's ephemeral one (same root cause as wave 1, all 6 spawn sites).
- *ROS 2 python servers never started* — `add_two_ints_server` /
  `action_server` used `python3 -c '<\n-escaped>'`, a `SyntaxError` (the `\n` is
  not a newline outside a string). Replaced with a quoted heredoc
  (`python3 - <<'NROS_PYEOF'`) so real newlines reach python.
- *Node INFO logs invisible* — the example nodes log `Received:` / `Response:` at
  INFO; the tests didn't set `RUST_LOG`, so the pattern match saw nothing. Added
  `RUST_LOG=info`.

With those fixed, **all four service + pub/sub directions hard-assert green**
against real `rmw_fastrtps` ROS 2:
- `test_xrce_to_ros2_pubsub`  — nano-ros XRCE talker → ROS 2 DDS listener ✅
- `test_ros2_to_xrce_pubsub`  — ROS 2 DDS talker → nano-ros XRCE listener ✅
- `test_xrce_service_ros2_client` — ROS 2 client → nano-ros XRCE server (`sum=8`) ✅
- `test_ros2_service_xrce_client` — nano-ros XRCE client → ROS 2 server (`5+3=8`) ✅

**Wave 3 (DONE) — actions interop both directions.** `test_xrce_action_ros2_client`
and `test_ros2_action_xrce_client` are now **hard asserts** against a real
`rmw_fastrtps` ROS 2 action server/client: goal **accept + feedback** round-trip
both ways (full Fibonacci sequence `[0,1,1,…,55]` streamed). Two bugs, both in
the action runtime (Rust `nros-node`), not the CDR header:

1. *Per-channel entity types.* `executor/action.rs` + `node.rs` advertised every
   action sub-entity with the bare action type (`…Fibonacci_`). ROS 2 matches the
   send_goal / get_result **services** by their per-channel service types and the
   feedback **topic** by `…Fibonacci_FeedbackMessage_`. Fixed: derive
   `…Fibonacci_SendGoal_` / `…Fibonacci_GetResult_` from the request envelope
   (`action_core::action_service_base_type`) and use `FeedbackMessage::TYPE_NAME`
   for feedback (cancel_goal/status already used `action_msgs/*`). ROS 2 now
   discovers all 5 entities (verified via `ros2 topic/service list -t`).

2. *GoalId wire format (protocol-wide).* `write_goal_id`/`read_goal_id` framed the
   goal UUID as a `u32(16)` sequence prefix + 16 bytes (20 B). ROS 2
   `unique_identifier_msgs/UUID` is a fixed `uint8[16]` (16 B, **no** prefix), so a
   real `rcl_action` peer rejected the 28-byte send_goal request (Fast-DDS
   `RTPS_READER_HISTORY` payload-size error) and nano-ros mis-read inbound goal
   ids. Fixed to fixed-16 everywhere: `write_goal_id`/`read_goal_id`, the
   hand-rolled skips in `handles.rs`, the FFI feedback offset in `arena.rs`,
   `GoalId::SEQ_PREFIX_LEN` → `0` (auto-corrects the nros-c / nros-cpp framing
   calcs), and the Cyclone `strip_feedback_goal_id_prefix` /
   `insert_goal_id_len_at` adapter removed (the runtime now emits the IDL-correct
   fixed array directly). nano-ros↔nano-ros stays self-consistent (both ends
   fixed-16); Cyclone `feedback_roundtrip` updated + green.

The example action client also now uses `wait_for_action_server` (spins discovery)
instead of a blind warmup, so the first send_goal doesn't race the
requester↔replier DDS match (volatile pre-match samples are lost otherwise).

**get_result deferral — DONE in [phase-237](phase-237-deferred-get-result.md).**
`rclcpp_action` sends `get_result` right after acceptance and expects the reply
only once the goal terminates; the server now holds the request (keyed by its
backend `sequence_number`) and flushes it on `complete_goal`. Forward
`test_xrce_action_ros2_client` hard-asserts the final `SUCCEEDED` result.
Implemented concurrent-safe (Option A) across all three service backends — XRCE +
Zenoh seq-keyed reply tables, Cyclone already native — so several goals can hold a
`get_result` at once under load. nano-ros↔nano-ros is unaffected (its client sends
get_result only after the goal terminates → immediate reply). Validated e2e over
both transports (`rmw_fastrtps`/XRCE incl. a 2-client concurrent test, and
`rmw_zenoh_cpp`/Zenoh) — see phase-237.

#### Design (wave 1, landed)
The CDR-header strip/prepend (233.5.1) covers **topics** (`publisher.c` /
`subscriber.c`). The **service** path (`service.c`) still carries the executor's
4-byte CDR encapsulation header on the XRCE wire. Self-consistent
(nano-ros↔nano-ros services/actions pass: `test_xrce_service_request_response`,
`test_xrce_action_fibonacci`) but will **not interop with real ROS 2
service/action endpoints**, exactly as topics didn't before 233.5.1. Off the PX4
critical path (PX4 is topic-only); needed for ROS 2 service/action interop.

#### Design

**Same fix as topics, applied to the 5 service wire-crossings.** The executor
serializes request/reply with `CdrWriter::new_with_header`, so the 4-byte CDR LE
header (`00 01 00 00`) is always at **payload byte 0**. The XRCE `SampleIdentity`
(24 B, request↔reply correlation) is **orthogonal** — it rides in the XRCE
REQUEST/REPLY submessage, a separate `request_callback` parameter from the
payload `ub`, handled by micro-XRCE + the agent. So the header sits at byte 0 of
the payload, with no framing in front of it; strip/prepend at byte 0, identical
to the topic fix.

- **Outbound — strip the 4-byte header (3 sites):**
  - `xrce_service_send_request_raw` → before `uxr_buffer_request` (`service.c:314`)
  - `xrce_service_call_raw` (blocking) → before `uxr_buffer_request` (`service.c:493`)
  - `xrce_service_send_reply` → before `uxr_buffer_reply` (`service.c:277`)

  Advance `data += 4`, `len -= 4` when `len >= XRCE_CDR_HEADER_LEN` (guarded, as
  in `publish_raw`).

- **Inbound — re-prepend the 4-byte header (2 sites):**
  - `xrce_request_callback` inbox write (`service.c:53`)
  - `xrce_reply_callback` inbox write (`service.c:85`)

  Write the CDR-LE header into `slot->data[0..4]`, then the payload; set
  `slot->len = len + 4`; bound-check `len + 4 <= XRCE_BUFFER_SIZE` (overflow path
  unchanged). Mirrors `xrce_topic_callback`.

- **Actions: no separate XRCE C path.** `nros-rmw-xrce` has no action code —
  actions ride on services (send_goal / cancel_goal / get_result via `call_raw` +
  service reply) and topics (feedback / status). The topic channels are already
  fixed (233.5.1); the service channels inherit the 3 strip + 2 prepend sites
  above. So fixing `service.c` makes actions interop too — verify the full
  goal → feedback → status → result round-trip rather than patching anything new.

- **Edge cases:**
  - *Empty request/reply* (e.g. `Trigger`/`Empty`): the executor emits just the
    4-byte header (no body). Strip → 0-byte XRCE payload; the agent re-adds the
    DDS representation header for ROS 2. Inbound 0-byte → prepend → 4-byte header,
    deserializes as empty. The `len >= 4` strip guard + `len + 4` prepend handle
    this; confirm with an empty-request service test.
  - *Endianness:* nano-ros always emits CDR-LE; the prepend writes the LE id. A
    big-endian ROS 2 peer is out of scope (nano-ros is LE-only elsewhere too).

- **Acceptance test:** a real `rmw_fastrtps` ROS 2 service client/server talks to a
  nano-ros XRCE service server/client over a `MicroXRCEAgent` — both directions —
  mirroring `packages/testing/nros-tests/tests/xrce_ros2_interop.rs` (gated on
  `require_ros2_dds()`, diagnostic-style). The nano-ros↔nano-ros tests can't catch
  this class (both sides consistently header-wrapped). Add an empty-request case.

- **Risk:** low and contained — 5 localized edits mirroring a landed, tested
  pattern; nano-ros↔nano-ros services/actions stay symmetric (regression-guarded
  by the existing `xrce` group). The only unknown is whether the agent does any
  extra request/reply payload framing beyond `SampleIdentity`; the topic result
  (agent passes the bare CDR body) strongly suggests not, but the interop test is
  the proof.

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
