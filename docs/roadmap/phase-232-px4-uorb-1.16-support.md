# Phase 232 — PX4 uORB support for 1.16+ (message versioning)

**Goal.** Keep the in-firmware uORB path (`nros-rmw-uorb` + the `px4-rs` submodule)
working on PX4 1.16+, where message versioning moved the core ROS 2 interface
topics into `msg/versioned/`. This is **Track A** of the two-track PX4 plan in
**RFC-0039** ("support both uORB and XRCE as first-class") — the *reactive* track
that tracks PX4 releases. RFC-0011 owns the backend internals.

**Status.** Substantially complete (2026-06). Done: **232.1** (msg/versioned/
enumeration — the blocker), **232.2** (MESSAGE_VERSION on the model), **232.4**
(6-field orb_metadata), **232.4b** (repair the uORB RMW vtable — it was
non-compiling vs the current ABI), **232.5** (pin stable **v1.17.0** + px4-rs
supported-window note). px4-rs `main` at `0f45e83`; PX4-Autopilot pinned to
v1.17.0 (`d6f12ad`); `cargo xtask gen-msgs` emits 235 messages incl. the
versioned core topics; the uORB C++ backend builds + `register_smoke` passes.
Remaining: **232.3** (optional FNV hash) + **232.6** (stale — no `topics.toml`
exists; needs re-scoping). Design-of-record: RFC-0039 (Draft) + RFC-0011 (Stable).

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

### 232.4 — Refresh the fallback `orb_metadata`  ✅ (nano-ros, in-tree)
`packages/px4/nros-rmw-uorb/src/uorb_abi.hpp` declares a pre-1.16 tail (`o_fields`).
Update the standalone fallback struct to the 6-field v1.16 layout (`o_name`,
`o_size`, `o_size_no_padding`, `message_hash`, `o_id`, `o_queue`). Harmless today (the
backend reads only the 3-field prefix; real module builds shadow it with PX4's
header) but correct + future-proof.
Done: `uorb_abi.hpp` now mirrors PX4 1.16+'s 6-field `orb_metadata`
(`orb_id_size_t = uint16_t`); `register_smoke`'s `kFakeMeta` updated. The C++
backend builds + the smoke test passes.
- **Files:** `packages/px4/nros-rmw-uorb/src/uorb_abi.hpp`,
  `tests/register_smoke.cpp`.

### 232.4b — Repair the uORB RMW vtable vs the current ABI  ✅ (nano-ros, found while validating 232.4)
The uORB C++ vtable no longer compiled against `nros_rmw_vtable_t`: the service
`create_*` slots gained a `const nros_rmw_qos_t*` param the stubs lacked, and the
positional initializer skipped Phase-130 `send_request_raw`/`try_recv_reply_raw`,
shifting every later slot (hard error). Fixed: qos param on the two UNSUPPORTED
service-create stubs + gap-free positional init through `call_raw` (rest NULL via
C++14 aggregate value-init; designated init isn't available at C++14). The whole
uORB backend was non-compiling before this — pre-existing RMW-ABI drift, not PX4
versioning.
- **Files:** `src/{vtable,service}.cpp`, `src/internal.hpp`.

### 232.5 — Pin a stable PX4 tag  ✅ (nano-ros + px4-rs)
The current `v1.17.0-alpha1` pin risks a silent uORB ABI break (raw `repr(C)`). Pin
the latest *stable* PX4 release in `nros-sdk-index.toml` (`source.px4-autopilot`);
record the supported window in `px4-rs` (`px4-sys` min + codegen parity note). Keep
`main` only in an opt-in forward-compat lane.
- **Files:** `nros-sdk-index.toml`; (px4-rs) `px4-sys` version doc.

### 232.6 — Resync `topics.toml` ↔ `dds_topics.yaml`  ⚠️ STALE — needs re-scoping (nano-ros, in-tree)
**Premise is stale:** there is no `topics.toml` anywhere under `packages/px4/`
(nor any `dds_topics`/topics-map file). The ROS↔uORB mapping is handled in the
px4-rs codegen instead — generated topics carry a `TOPICS` const + resolve their
canonical `orb_metadata` at runtime via `px4_rs_find_orb_meta()` (see any
`crates/px4-msg-codegen/generated/*.rs`). So either this item refers to a file
that was never created / since removed, or the resync belongs in px4-rs. Revisit
when 232.5 picks a pin — confirm whether nano-ros needs a static ROS↔uORB map at
all, or whether the px4-rs canonical-resolution path already covers it.
- **Files (if revived):** TBD — no `topics.toml` exists today.

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
