# Phase 232 — PX4 uORB support for 1.16+ (message versioning)

**Goal.** Keep the in-firmware uORB path (`nros-rmw-uorb` + the `px4-rs` submodule)
working on PX4 1.16+, where message versioning moved the core ROS 2 interface
topics into `msg/versioned/`. This is **Track A** of the two-track PX4 plan in
**RFC-0039** ("support both uORB and XRCE as first-class") — the *reactive* track
that tracks PX4 releases. RFC-0011 owns the backend internals.

**Status.** In progress (2026-06). The px4-rs codegen jobs **232.1 (the blocker)
+ 232.2** are done on the px4-rs branch `phase-232-uorb-versioned-msgs` (pushed;
PR open) — `cargo xtask gen-msgs` now emits 246 messages (was 209), including all
37 `msg/versioned/` core topics, verified against `v1.17.0-alpha1`. The nano-ros
submodule pointer bumps once that branch merges to px4-rs `main`. Remaining: the
in-tree jobs 232.4/232.5/232.6 + optional 232.3. Design-of-record: RFC-0039
(Draft) + RFC-0011 (Stable).

**Priority.** P1 — without item 232.1 the offboard/telemetry topics
(`VehicleOdometry`, `VehicleCommand`, `VehicleLocalPosition`, `VehicleAttitude`,
`TrajectorySetpoint`, `BatteryStatus`) **do not generate** on the current
PX4-Autopilot pin (`v1.17.0-alpha1`); 37 topics are `msg/versioned/`-only.

**Depends on.** RFC-0039 (umbrella + decision), RFC-0011 (uORB backend), the
`px4-rs` submodule (`third-party/px4/px4-rs`, maintained out-of-tree — fork-push
workflow), the PX4-Autopilot pin (`nros-sdk-index.toml` `source.px4-autopilot`).

## Overview

PX4 1.16 introduced versioned messages: `msg/versioned/*.msg` carry
`uint32 MESSAGE_VERSION = N`, the build computes `orb_metadata.message_hash`
(FNV-1a), and a companion-side translation node handles cross-version matching.
For the **in-firmware uORB path** the translation node is *pre-agent* and does not
help — the raw `#[repr(C)]` structs must match the firmware exactly. px4-rs is
largely version-robust already (its generated `metadata()` resolves PX4's
canonical `__orb_<name>` via `px4_rs_find_orb_meta`, real hash and all), so the
work is narrow: enumerate the versioned `.msg`, track the version, and refresh the
nano-ros-side fallback ABI.

## Architecture

```
PX4 .msg tree (pin)              px4-rs                         in-firmware
msg/*.msg + msg/versioned/*.msg ─► px4-msg-codegen ─► Rust types + orb_metadata
                                   (xtask enumerates + search_path)
                                            │ metadata() → px4_rs_find_orb_meta(name)
                                            ▼ canonical __orb_<name> (real hash)
                                   nros-rmw-uorb (RFC-0011) ─► uORB pub/sub
```

## Work Items

### 232.1 — Enumerate `msg/versioned/` (the blocker)  ✅ (px4-rs)
The xtask (`third-party/px4/px4-rs/crates/.../xtask/src/main.rs:136-160`) reads only
`<px4>/msg/*.msg` and search-paths `[msg/]`. Extend it to also enumerate
`<px4>/msg/versioned/*.msg` and add `msg/versioned/` to the codegen `search_path`
(so nested-type resolution finds versioned types). Verify the 37 versioned topics
generate (spot-check `VehicleOdometry`, `VehicleCommand`).
- **Files (px4-rs):** xtask msg enumeration + search-path construction.
- **Acceptance:** the core control/telemetry topics generate; `cargo build` of a
  module that uses `VehicleOdometry`/`VehicleCommand` links.

### 232.2 — Track `MESSAGE_VERSION`  ✅ (px4-rs)
`px4-msg-codegen` parses `MESSAGE_VERSION = N` as a plain constant. Capture it on
the message model (e.g. `message_version: Option<u32>`) so version-aware tooling
(compat checks, emitting the version) can use it. The generated `pub const
MESSAGE_VERSION` may stay.
- **Files (px4-rs):** `px4-msg-codegen` parser + model.

### 232.3 — FNV-1a `message_hash` for custom topics (optional)  ⬜ (px4-rs)
Standard topics already get PX4's real hash via canonical resolution; the synthesized
`message_hash = 0` only applies to user-introduced topics absent from PX4's table. If
such a topic is exported to ROS 2 via the DDS client, compute FNV-1a (seed
`0x811c9dc5`, prime `0x1000193`, over the flattened field string) to match PX4. Low
urgency.
- **Files (px4-rs):** `px4-msg-codegen` emit + a hash module.

### 232.4 — Refresh the fallback `orb_metadata`  ⬜ (nano-ros, in-tree)
`packages/px4/nros-rmw-uorb/src/uorb_abi.hpp` declares a pre-1.16 tail (`o_fields`).
Update the standalone fallback struct to the 6-field v1.16 layout (`o_name`,
`o_size`, `o_size_no_padding`, `message_hash`, `o_id`, `o_queue`). Harmless today (the
backend reads only the 3-field prefix; real module builds shadow it with PX4's
header) but correct + future-proof.
- **Files:** `packages/px4/nros-rmw-uorb/src/uorb_abi.hpp` (+ the header doc-comment).

### 232.5 — Pin a stable PX4 tag  ⬜ (nano-ros, in-tree)
The current `v1.17.0-alpha1` pin risks a silent uORB ABI break (raw `repr(C)`). Pin
the latest *stable* PX4 release in `nros-sdk-index.toml` (`source.px4-autopilot`);
record the supported window in `px4-rs` (`px4-sys` min + codegen parity note). Keep
`main` only in an opt-in forward-compat lane.
- **Files:** `nros-sdk-index.toml`; (px4-rs) `px4-sys` version doc.

### 232.6 — Resync `topics.toml` ↔ `dds_topics.yaml`  ⬜ (nano-ros, in-tree)
On the pin chosen in 232.5, resync `packages/px4/nros-rmw-uorb/topics.toml` to the
firmware's `src/modules/uxrce_dds_client/dds_topics.yaml` (the ROS-name ↔ uORB-name
map), including any versioned entries.
- **Files:** `packages/px4/nros-rmw-uorb/topics.toml`.

## Acceptance

- The 37 `msg/versioned/` topics generate; the core control/telemetry set links in a
  PX4 module build.
- `MESSAGE_VERSION` is captured on the message model (232.2).
- The standalone fallback `orb_metadata` matches the 6-field v1.16 layout.
- PX4-Autopilot pinned to a stable tag; `topics.toml` resynced.
- A uORB smoke/register test (extend `examples/px4/cpp/uorb/nros-register-check`)
  exercises a versioned topic.

## Notes

- **Ownership:** 232.1–232.3 land in the **px4-rs** submodule (out-of-tree;
  fork-push workflow — the agent prepares, the maintainer pushes; bump the
  superproject pointer only after the fork push). 232.4–232.6 are in this tree.
- px4-rs's canonical-metadata resolution (`px4_rs_find_orb_meta`) is why this phase
  is narrow rather than a hash-machinery rewrite — see RFC-0039.
- Parallel track: Phase 233 (XRCE companion path).
