---
id: 267
title: "XCDR1-FINAL vs ROS2 XCDR2-APPENDABLE gap: Control msg mis-walked after domain_bridge republish (nano-ros serializer PROVEN canonical; no XCDR2/DHEADER path)"
status: open
type: bug
severity: high
area: rmw
related: [rfc-0055, phase-303]
---

> **Fix tracked by [RFC-0055](../design/0055-wire-encoding-xcdr2-extensibility.md)
> + [phase-303](../roadmap/phase-303-xcdr2-interop.md)** — the XCDR2 + explicit
> extensibility workstream. This issue is the root-cause record + the byte-exact
> serializer guard; the code fix (DHEADER + declared extensibility + negotiation)
> lands there. Closes when the phase-303 "done when" (the domain_bridge republish
> survives) is met.

## Symptom (simple-autoware-safety-island demo, 2026-07-24)

The island (cyclone RMW) publishes `autoware_control_msgs/Control`
(`longitudinal.acceleration = -2.5`). A humble `ros2 topic echo` subscribing
DIRECTLY (same domain) decodes it correctly — proven repeatedly. But when
`ros2 domain_bridge` (humble branch, GenericSubscription/GenericPublisher
serialized passthrough) rebroadcasts the SAME topic into another domain, the
downstream typed subscriber decodes garbage:

    longitudinal.acceleration = 2677354240.0   (≈ 0x9F99999A — bytes of the
    real payload shifted; -2.5f is 0xC0200000, 0.3f is 0x3E99999A)

`autoware_adapi_v1_msgs/MrmState` (flat: Time + 2×uint16) crosses the same
bridge CLEAN. `Control` nests Lateral/Longitudinal each with TWO
builtin_interfaces/Time members — the shifted-float signature points at a
CDR alignment divergence in the nested-struct layout that a typed cyclone
reader tolerates (or realigns) but a serialized-passthrough rebroadcast
preserves verbatim into a payload the next typed reader mis-walks.

Live impact: the demo's sim-side vehicle_cmd_gate consumed the garbage
emergency command and accelerated the vehicle to the 50 m/s cap.

## Repro sketch

1. Island (nano-ros cyclone, domain 2) publishing Control.
2. `ros2 run domain_bridge domain_bridge` with a 2→1 row for the topic.
3. Domain 1: `ros2 topic echo /system/emergency/control_cmd` → garbage;
   domain 2 direct echo → clean.

## Root-cause investigation (2026-07-25) — the serializer is NOT the bug

The original suspect (nano-ros CDR padding for nested structs) is **DISPROVEN**
by a byte-exact test. `nros-serdes::compat_tests::
test_control_nested_struct_time_bool_layout_0267` serializes the EXACT Control
field sequence (Time + Lateral{Time,f32,f32,bool} + Longitudinal{Time,f32×3,
bool,bool}) the generated `serialize` emits and asserts every member lands at
its canonical XCDR1 offset:

- Lateral's trailing bool @24, then the correct 3-byte i32-alignment pad
  (25→28), `Longitudinal.stamp.sec` @28 — the exact boundary this issue
  suspected is CANONICAL.
- `Longitudinal.acceleration` (the demo's corrupted -2.5) lands @40 as
  `0x00 00 20 C0` — canonical `-2.5f`. Total length 50.

So nano-ros emits **canonical XCDR1** bytes (consistent with "direct typed echo
clean"), and the CDR writer's `align` is standard relative-to-origin. The
corruption is introduced DOWNSTREAM, not by `nros-serdes`.

## Real root cause — XCDR1-FINAL vs ROS 2 XCDR2-APPENDABLE representation gap

nano-ros emits ONLY the `0x0001` PLAIN_CDR_LE encapsulation (XCDR1 FINAL);
`nros-serdes` has NO XCDR2 / CDR2 / DHEADER path, and `nros-msg-to-idl` emits
NO explicit extensibility (`@final`/`@appendable`) in the generated IDL. ROS 2
(humble+) types are `@appendable` by default, and under XCDR2 an appendable
nested struct is prefixed with a 4-byte **DHEADER** (its serialized size).

The shifted-float signature is exactly what a phantom-DHEADER misparse
produces: a downstream reader using the type's XCDR2/appendable typesupport
consumes a 4-byte DHEADER that nano-ros's XCDR1-FINAL stream does not contain,
shifting every subsequent nested-struct member. The DIRECT reader decodes clean
because it reads nano-ros's `0x0001` header and decodes as XCDR1-FINAL (no
DHEADER); `domain_bridge`'s GenericSubscription/Publisher re-publish crosses a
representation boundary where the downstream uses XCDR2 for the appendable type.

## Fix directions (both need the live demo to verify; neither safe to land blind)

1. **Support XCDR2 + DHEADER for appendable/mutable types** in `nros-serdes`
   (the real fix; a substantial serdes feature — encoding version negotiation +
   DHEADER emit for nested appendable structs).
2. **Pin extensibility** by emitting explicit `@final` in `nros-msg-to-idl` so
   no reader ever expects a DHEADER. CAUTION: this changes the RIHS type hash
   and diverges from ROS 2's `@appendable` Control — it could break the
   currently-working direct-match path, so it must be verified against the live
   demo before landing (do NOT apply blind).

Until then the byte-exact test guards the serializer against a genuine future
CDR regression, and the demo workaround (single-bridge topology) stays.

## Update (phase-303 W1, 2026-07-25) — nano-ros's cyclone IDL MATCHES ROS 2 Humble

A first fix attempt (emit `@appendable` in the generated IDL) was reverted:
`nros-msg-to-idl` is byte-parity-locked to ROS 2's own `rosidl_adapter`, and the
Humble reference `.idl`s carry NO extensibility annotation. So nano-ros already
produces the SAME `.idl` as Humble → the same cyclone descriptor → the same wire
extensibility as a native Humble node. This SHARPENS the diagnosis: on a
pure-Humble graph there is no `.idl`-layer mismatch, so the corruption implies
the downstream is NOT pure-Humble (a newer-distro reader decoding Humble XCDR1
data as XCDR2/appendable), OR nano-ros's *vendored* idlc default extensibility
diverges from the target's. **Next actionable step (phase-303 W1): capture the
demo's downstream ROS distro, its negotiated `data_representation`, and both
descriptors' extensibility** before any code fix. See RFC-0055 §"Finding
(phase-303 W1)".

## Update (phase-303 W1 continued, 2026-07-26) — offline diagnosis resolves the LAYER + locates BOTH control points

Gathered decisive OFFLINE evidence (no live demo), which retires one fix
direction and pinpoints the code:

1. **The `.idl` annotation is the WRONG layer — direction #2 is RETIRED.**
   Humble (host `/opt/ros/humble`) AND Jazzy (`ros:jazzy-ros-base` container)
   ship the SAME unannotated `.idl` for nested-`Time` types (`std_msgs/Header`,
   `geometry_msgs/PoseStamped`): plain `struct Header {` with only `@verbatim`
   comment annotations, NO `@final`/`@appendable`. So emitting `@final` (or
   `@appendable`) in `nros-msg-to-idl` would diverge from BOTH distros and break
   the `rosidl_adapter` parity contract — it fixes nothing and regresses parity.

2. **nano-ros's cyclone descriptor is FINAL by construction — located.** The
   cyclone path does NOT compile the `.idl` via `idlc` at build time; it builds
   the `dds_topic_descriptor_t` at RUNTIME in
   `packages/dds/nros-rmw-cyclonedds/bridge/dynamic_type_builder.cpp`. There,
   `m_flagset` (≈ line 1068) is set to ONLY `DDS_TOPIC_FIXED_SIZE` (or `0`) —
   never an appendable/XCDR2 flag — and the emitted ops array carries no
   `DDS_OP_DLC` (delimited/DHEADER) op. **⇒ nano-ros publishes a FINAL,
   no-DHEADER, XCDR1-shaped descriptor by construction.** (Aside: neither ROS
   distro sets `IDLC_DEFAULT_EXTENSIBILITY` in its `idlc` CMake path, and the
   vendored idlc's own default is `UNDEFINED → IDL_FINAL` — but that route is
   irrelevant to nano-ros, which uses the runtime builder above; the ROS peer's
   wire extensibility comes from its `rosidl_typesupport`/rmw sertype, a
   separate mechanism.)

3. **Mechanism confirmed as extensibility+representation, not `.idl`.** The
   corruption's phantom-4-byte-DHEADER float shift means the DOWNSTREAM reads
   `Control` as APPENDABLE under XCDR2 (expects a DHEADER nano-ros's FINAL/XCDR1
   stream never wrote). Direct same-domain echo is clean because it honors the
   `0x0001` (XCDR1) encapsulation; the `domain_bridge` GenericPublisher crosses
   into an XCDR2/appendable decode.

**⇒ Evidence-based fix plan (both edition-gated by RFC-0056's
`[system].ros_edition` — the axis the earlier "unknown downstream distro"
blocker is now resolved by, since the user DECLARES the target edition):**

- **Cyclone path:** teach `dynamic_type_builder.cpp` to emit an APPENDABLE
  descriptor — set the appendable extensibility in `m_flagset` AND emit
  `DDS_OP_DLC` delimiter ops so Cyclone writes a DHEADER per nested appendable
  struct (Cyclone does XCDR2/DHEADER natively once the descriptor declares it).
  Humble edition stays FINAL (byte-identical). This is a DESCRIPTOR-BUILDER
  change — NO `nros-msg-to-idl`/`.idl` parity impact.
- **`nros-serdes` path** (zenoh-pico / XRCE / native-Rust): the XCDR2 writer +
  reader + DHEADER of phase-303 W2–W3.

**Remaining live-demo capture (now the ONLY open item, narrowed):** confirm the
demo's downstream reads `Control` as appendable-under-XCDR2 — i.e. dump its
negotiated `data_representation` + the runtime rmw sertype extensibility for
`autoware_control_msgs/Control`. The offline evidence makes this a confirmation,
not an exploration. Do not land the descriptor/serdes change before it (per the
no-blind-land rule), but the DIRECTION and the CODE are now fixed.

## Update (W1c LANDED, 2026-07-26) — cyclone descriptor emits APPENDABLE on iron/jazzy+

The cyclone-path half of the fix shipped (phase-303 W1c).
`dynamic_type_builder.cpp` now takes an `extensibility` argument: iron/jazzy+
(edition-gated via `dynamic_type::TYPE_EXTENSIBILITY`) prefixes each aggregate's
op stream with `DDS_OP_DLC`, so Cyclone tags the type APPENDABLE and writes a
per-nested-struct DHEADER under XCDR2 — matching a modern peer. Humble stays
FINAL, byte-identical. Verified against the real `libddsc.a`
(`tests/appendable_extensibility.cpp`): FINAL = 0 DLCs; APPENDABLE = 1 per
aggregate (nested type → 2). No `.idl`/parity impact. The `nros-serdes` XCDR2
half (zenoh/XRCE/native paths) is still W2/W3. End-to-end wire delivery (the
domain_bridge demo clearing) is the remaining live-demo confirmation.

## Update (W2/W3 serdes core LANDED, 2026-07-26) — nros-serdes XCDR2 machinery

The `nros-serdes` half of the fix (for the zenoh-pico / XRCE / native-Rust paths,
which don't go through Cyclone) got its core: `CdrWriter`/`CdrReader` now support
XCDR2 (DELIMITED_CDR2, `0x0009`) with `begin/end_dheader` — the writer
reserves+backpatches a 4-byte DHEADER per struct; the reader reads+bounds by it
and skips unknown trailing members (forward compat); 8-byte primitives align to 4.
Under XCDR1 the DHEADER calls are NO-OPs, so it's byte-identical to today.
Byte-exact + round-trip tests green. Remaining (phase-303 W4): wrap the generated
serialize/deserialize in the DHEADER calls + have the RMW select the XCDR2 writer
by edition/negotiation — landed together so the path is wire-verifiable against a
live Jazzy peer at once. The Cyclone path (the demo's actual corruption) is
already fixed by W1c.

## Update (W4 codegen-wrap + tx-writer gate LANDED, 2026-07-26)

The nros-serdes XCDR2 machinery is now wired into the Rust path: generated
serialize/deserialize wrap every struct in DHEADER calls (no-op XCDR1), and
`nros_node::tx_writer` selects the XCDR2 writer on iron/jazzy/rolling (humble
byte-identical). The reader auto-detects the encapsulation version. nros-node
green under both editions. Remaining: the C/C++ tx writers (`nros-c`/`nros-cpp`)
need the same gate, and the live-peer wire round-trip is the final confirmation.
The Cyclone path (the demo's corruption) is already fixed by W1c.

## Suspect (original — superseded by the investigation above)

nano-ros CDR serializer's padding for nested structs w/ Time members
(4+4 bytes) vs rosidl's XCDR1 alignment rules — a typed reader may
resynchronize while byte-level rebroadcast exposes the divergence. Compare
`nros-serdes` output byte-for-byte with `rmw_cyclonedds_cpp` for
autoware_control_msgs/Control; check encapsulation header + 8-byte-alignment
of float64s... (Control has only float32 — suspect the bool tail of Lateral
(`is_defined_steering_tire_rotation_rate`) + struct padding before
Longitudinal).

## Workaround in the demo

Single-bridge topology (fault = whole-bridge pause) instead of the split
forward/reverse topology that would have kept the island's commands flowing
through the rebroadcast path.
