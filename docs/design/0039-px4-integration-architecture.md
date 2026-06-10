---
rfc: 0039
title: "PX4 integration architecture (uORB in-firmware + companion XRCE; 1.16 message versioning)"
status: Draft
since: 2026-06
last-reviewed: 2026-06
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# RFC-0039 — PX4 integration architecture

## Summary

PX4 is a first-class nano-ros target, but the integration has two distinct
shapes that have not been described together, and PX4 v1.16 introduced **message
versioning** that changes the compatibility story. This RFC is the umbrella for
PX4 support: it (1) describes PX4's own ROS 2 model (uXRCE-DDS client/agent +
v1.16 versioning), (2) places nano-ros in it as **two positions** — *in-firmware
uORB* (`nros-rmw-uorb`, the existing path) and *companion/peer XRCE*
(`nros-rmw-xrce`, latent), (3) records the `px4-rs` submodule's role and its
version-robustness mechanism, and (4) lists the concrete revision opportunities
for PX4 1.16+. RFC-0011 remains the detail spec for the `nros-rmw-uorb` backend;
this RFC is the strategy around it.

## Motivation / problem

- nano-ros ships a uORB RMW backend (RFC-0011) that runs *inside* PX4 firmware,
  but PX4's *mainstream* ROS 2 integration is the **companion uXRCE-DDS** path —
  and nano-ros has an XRCE-DDS client backend (`nros-rmw-xrce`) that already fits
  it, with no example and no recorded strategy.
- PX4 v1.16 added **message versioning** (`msg/versioned/`, a `MESSAGE_VERSION`
  field, an `orb_metadata.message_hash`, and a companion-side
  `px4_ros2_msg_translation_node`). The two nano-ros positions are affected very
  differently, and the `px4-rs` codegen must track the change.
- The PX4-Autopilot pin (`third-party/px4/PX4-Autopilot`) is already
  `v1.17.0-alpha1` — the post-versioning era — so this is current, not
  speculative.

## Design

### PX4's ROS 2 model (external, for reference)

```
PX4 firmware                      companion computer                ROS 2
uxrce_dds_client  ──serial/UDP──►  Micro XRCE-DDS Agent  ──DDS──►  px4_msgs nodes
(eProsima XRCE client)             (eProsima; PX4-independent)
```

- **Topic map:** `src/modules/uxrce_dds_client/dds_topics.yaml` pairs each uORB
  topic with a `px4_msgs::msg::*` type. `/fmu/out/*` = PX4→ROS, `/fmu/in/*` =
  ROS→PX4. (Replaced the pre-v1.14 microRTPS bridge.)
- **QoS (must match or no comms):** PX4 publishers are **TRANSIENT_LOCAL +
  BEST_EFFORT + KEEP_LAST**. Default ROS 2 QoS (reliable + volatile) is
  incompatible.
- **v1.16 message versioning:** versioned defs live in `msg/versioned/` with a
  `uint32 MESSAGE_VERSION = N` constant; the build computes an
  `orb_metadata.message_hash` (FNV-1a over the flattened field string); a
  companion-side `px4_ros2_msg_translation_node` converts between the firmware's
  built-in version and the ROS 2 app's version, decoupling app message version
  from firmware. `subscriptions_multi` + `route_field` demux one ROS topic into
  multiple uORB instances.

### The two nano-ros positions

| position | backend | role | wire | agent? | versioning exposure |
|---|---|---|---|---|---|
| **in-firmware** | `nros-rmw-uorb` (RFC-0011) | a nano-ros module *inside* PX4, on uORB directly | raw `#[repr(C)]` memcpy | no (in-process) | **brittle** — must match the firmware's exact uORB struct; the translation node is pre-agent and does not help |
| **companion / peer** | `nros-rmw-xrce` | an embedded nano-ros node on a peer, talking px4_msgs to the same agent | CDR over XRCE | yes (MicroXRCEAgent) | **buffered** — the translation node lets the app use a px4_msgs version independent of firmware |

Both are legitimate and serve different niches; nano-ros is unusual in being able
to occupy *either*. The in-firmware path is the perf/zero-copy niche PX4's own ROS
2 push does not touch; the companion path is the mainstream PX4↔ROS 2 integration.

**Decision (2026-06): support both as first-class.** This is not a transitional
state — nano-ros commits to *both* PX4 positions. The two have opposite version
philosophies, which sets their maintenance contracts:

- **uORB (in-firmware)** is raw-ABI-rigid → it needs a **stable pin** (OQ3) and
  must keep pace with PX4's `.msg` reorganisation (the `msg/versioned/` blocker,
  #1) just to keep generating the live topic set. Maintenance is *reactive to PX4
  releases*.
- **XRCE (companion)** is version-buffered by the agent translation node → it is
  comparatively turnkey but currently **untapped** (no example, no px4_msgs CDR
  emit). Work here is *additive*, not reactive.

So the roadmap carries two parallel tracks: keep uORB working on new PX4 (the
codegen fixes), and stand up the XRCE companion path (codegen emitter + example).

### `px4-rs` and the version-robustness mechanism

The in-firmware path rests on the `px4-rs` submodule (`third-party/px4/px4-rs`,
maintained out-of-tree): `px4-msg-codegen` (`.msg` → Rust types + synthesized
`orb_metadata`), `px4-uorb` (typed `Publication`/`Subscription`), `px4-workqueue`
(async on PX4's WorkQueue), `px4-sys` (FFI; its `orb_metadata` binding already
carries `message_hash: u32`).

The key robustness mechanism: a generated topic's `metadata()` resolves PX4's
**canonical** `__orb_<name>` at runtime via `px4_rs_find_orb_meta(name)`, and
only falls back to the codegen-synthesized copy (with `message_hash = 0`) when PX4
does not know the topic (a user-introduced topic, or the host mock). So for every
standard PX4 topic, nano-ros uses PX4's real metadata — real `message_hash`, real
`o_id` — automatically. **This is why most of the v1.16 hash concern is already
handled** for standard topics.

### PX4 1.16+ `orb_metadata` ABI (the 6-field contract)

```c
struct orb_metadata {
    const char    *o_name;
    const uint16_t o_size;
    const uint16_t o_size_no_padding;
    uint32_t       message_hash;   // v1.16+: FNV-1a over fields
    orb_id_size_t  o_id;           // ORB_ID enum
    uint8_t        o_queue;
};
```

In a real module build (`NROS_RMW_UORB_LINK_PX4=ON`) the backend uses PX4's own
header (correct layout). The standalone fallback in
`packages/px4/nros-rmw-uorb/src/uorb_abi.hpp` declares only a prefix and is the
piece that must track this struct.

### Version strategy

- **Pin policy:** track a PX4 release/main pin in `nros-sdk-index.toml`
  (`source.px4-autopilot`); the supported window is recorded in `px4-rs`
  (`px4-sys` min version + `px4-msg-codegen` layout-parity note).
- **`topics.toml` ↔ `dds_topics.yaml`:** nano-ros's uORB `topics.toml` mirrors a
  snapshot of PX4's `dds_topics.yaml`; resync on a pin bump.
- **Versioned-message resolution:** v1.16 relocated live topics into
  `msg/versioned/`; the codegen must search there (see revision opportunity #1).

## Revision opportunities (PX4 1.16+)

Ordered by real impact (severity re-scoped after confirming the canonical-metadata
resolution):

1. **CONFIRMED BLOCKER — `msg/versioned/` is not enumerated.** The px4-rs xtask
   (`xtask/src/main.rs:136-160`) reads only `<px4>/msg/*.msg` (`read_dir`, no
   recursion) and passes search_path `[msg/]`. On the v1.17-alpha pin, **37 topics
   live in `msg/versioned/`** and are *removed from flat `msg/`* — including the
   core ROS 2 interface set (`VehicleOdometry`, `VehicleCommand`,
   `VehicleLocalPosition`, `VehicleAttitude`, `TrajectorySetpoint`,
   `BatteryStatus`). So px4-rs currently **cannot generate the offboard/telemetry
   topics** on PX4 1.16+. Fix: enumerate `msg/versioned/` too and add it to the
   search_path (for nested-type resolution). The single highest-priority item.
   (px4-rs.)
2. **MEDIUM — track `MESSAGE_VERSION`.** It is parsed as a plain constant; capture
   it on the message model for version-aware tooling. (px4-rs.)
3. **MEDIUM (custom topics only) — `message_hash = 0`.** Moot for standard topics
   (canonical resolution supplies the real hash); only relevant for user-introduced
   topics, and only if such a topic is exported to ROS 2 via the DDS client.
   Optionally compute FNV-1a (seed `0x811c9dc5`, prime `0x1000193`). (px4-rs.)
4. **LOW (this tree) — refresh the fallback `orb_metadata`.**
   `nros-rmw-uorb/src/uorb_abi.hpp` declares a pre-1.16 tail (`o_fields`); update
   to the 6-field v1.16 layout (`message_hash`, `o_id`, `o_queue`) for accuracy.
   Harmless today (3-field prefix only is read; real builds shadow it).
5. **LOW — companion XRCE example + QoS.** `nros-rmw-xrce` already supports
   `TRANSIENT_LOCAL` (`session.c`); add a peer-side px4_msgs-over-XRCE example and
   confirm BEST_EFFORT to interop with MicroXRCEAgent. This is the mainstream PX4
   ROS 2 path and currently has no example.

## Alternatives considered

- **Fold everything into RFC-0011.** Rejected — 0011 is a Stable, focused
  backend-internals spec; the cross-cutting strategy (two positions, versioning,
  px4-rs, companion path) is a different scope and belongs in its own RFC that
  cites 0011.
- **Drop the in-firmware path and chase only companion XRCE.** Rejected — the
  in-firmware zero-copy uORB niche is exactly what PX4's own ROS 2 stack does not
  provide; it is a differentiator.
- **Make the uORB path version-tolerant.** Not viable — uORB is raw `#[repr(C)]`
  memcpy with no translation layer; tolerance lives only on the companion XRCE
  path (the translation node).

## Open questions

1. ~~Does the px4-rs codegen pass `msg/versioned/`?~~ **Resolved: no.** The xtask
   (`xtask/src/main.rs:136-160`) enumerates only `<px4>/msg/*.msg` and search_paths
   `[msg/]`; the 37 versioned-only topics (incl. the core control/telemetry set) do
   not generate on 1.16+. Promoted to revision opportunity #1 (confirmed blocker).
2. **`px4_msgs` (CDR) for the XRCE path — one source, two emitters (recommended),
   not an external package.** Feed the *same* PX4 `.msg` tree (`msg/` +
   `msg/versioned/`) into both `px4-msg-codegen` (raw `repr(C)`, uORB) and nano-ros's
   `rosidl-codegen` (CDR, XRCE). Rationale: `px4_msgs` *is* generated from these
   files, so the format is compatible; one source keeps both emitters lock-stepped to
   the same PX4 pin with no external ament dependency. Caveats to resolve: (a)
   `rosidl-codegen` must accept the PX4-`.msg` `MESSAGE_VERSION` constant; (b) the
   `version` field is a normal payload field — generate it, and the agent-side
   translation node handles cross-version matching; (c) nano-ros's `type_hash`
   ("TypeHashNotSupported" on Humble) is orthogonal to PX4's `message_hash` — DDS
   matching is by topic+type name, so the XRCE path needs the right type *names*
   (`px4_msgs::msg::*`) and QoS, not the uORB hash.
3. **Pin policy — pin a stable PX4 *tag*, not `main` (recommended).** The current
   `v1.17.0-alpha1` pin is risky: the in-firmware uORB path is raw `repr(C)`, so an
   alpha `orb_metadata`/struct change is a silent ABI break. Pin the latest *stable*
   release (e.g. 1.16.x once tagged) for reproducible, deployable builds; bump on PX4
   releases; track `main` only in an opt-in forward-compat CI lane. The companion
   XRCE path tolerates version skew (translation node); the uORB path does not.

## Changelog

- 2026-06 — created (Draft). Umbrella for PX4 support: PX4's uXRCE-DDS + v1.16
  versioning model, nano-ros's in-firmware-uORB vs companion-XRCE positions,
  `px4-rs`'s canonical-metadata robustness, the 6-field `orb_metadata` ABI, and the
  1.16 revision opportunities. Detail for the uORB backend stays in RFC-0011.
  External refs: PX4 Guide — uXRCE-DDS + ROS 2 User Guide (v1.16/main).
- 2026-06 — resolved OQ1 (px4-rs xtask reads only `msg/`; 37 versioned-only topics,
  incl. the core control/telemetry set, do not generate on 1.16+ → promoted #1 to
  confirmed blocker); recommended one-source-two-emitters for OQ2 and a stable-tag
  pin for OQ3.
- 2026-06 — **decision: support both uORB and XRCE as first-class** (not
  transitional). Recorded the per-path maintenance contracts (uORB reactive to PX4
  releases; XRCE additive) and the two parallel roadmap tracks.
