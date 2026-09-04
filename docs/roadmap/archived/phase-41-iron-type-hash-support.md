# Phase 41: Iron+ Type Hash Support

**Status (2026-09-04): COMPLETE.** The residual closed 2026-07-27 via phase-311
W5 (below); what kept this doc open afterwards was fifteen unticked work-item
boxes describing work the header already said was done. Each is now ticked
against the code that satisfies it, verified on `main` 2026-09-04 — see the
note above 41.2 for why the delivered mechanism does not match the boxes'
wording.

*Original status line, 2026-07-26:* **Substantially complete.** The RIHS01 machinery is built,
wired, and verified end to end for iron + jazzy:
- **Computation** — `packages/cli/rosidl-resolve/src/rihs.rs` (the path below
  read `rosidl-codegen/src/rihs.rs`, which predates the codegen move into
  `packages/cli/`; the file is there, under a crate that was renamed) (`build_type_description` +
  `rihs01`, message/service/action) computes REP-2011 hashes; proven
  `engine == fixture == live Jazzy` by `packages/cli/rosidl-bindgen/tests/edition_hash_oracle.rs`.
- **Codegen** — `nros generate-rust --ros-edition {iron,jazzy}` emits the real
  per-message `const TYPE_HASH` (verified: `std_msgs/Int32` →
  `RIHS01_b6578ded3c58c626cfe8d1a6fb6e04f706f97e9f03d2727c9ff4e74b1cef0deb`,
  not the `RIHS01_0…0` placeholder). `--ros-edition jazzy` now parses (was
  rejected — `cargo-nano-ros::parse_ros_edition` only knew humble|iron; fixed to
  route through the `RosEdition::parse` SSoT).
- **Runtime** — `nros-node` threads `M::TYPE_HASH` → `TopicInfo` → the
  edition-gated zenoh keyexpr / liveliness (`nros-rmw-zenoh/src/keyexpr.rs`):
  humble → `TypeHashNotSupported`, iron/jazzy → the real hash.

**Residual — CLOSED (2026-07-27, via phase-311 W5).** The live-peer ZENOH interop
check now exists: `packages/testing/nros-tests/tests/ros_editions_e2e.rs` (the
zenoh cells; written here as `ros_editions_zenoh.rs`, which #327 collapsed into
that matrix rstest) runs a
`ros-jazzy` nano-ros zpico node against a stock jazzy `rmw_zenoh_cpp` peer over a
shared `rmw_zenohd`, and delivery only succeeds because the keyexpr RIHS01
type-hash tail matches (proven: a `ros-humble` build with the placeholder tail
does NOT deliver). pub/sub + service + action all interop both ways. The wire
loop for the zenoh path is closed — the offline+container oracle's hashes are now
also confirmed on the wire. (Issue #0291 established the zenoh transport itself is
version-compatible; #0292 fixed the action-server service-hash tail.)

**Priority: Low (residual now done)**
**Prerequisites:** Phase 16 (ROS 2 interop — complete for Humble)

> **Part of the ROS-edition axis ([RFC-0056](../design/0056-ros-edition-axis.md)).**
> RIHS01 type hash is one field of the per-edition interop profile; the wire
> encoding / extensibility field is [RFC-0055](../design/0055-wire-encoding-xcdr2-extensibility.md)
> / [phase-303](phase-303-xcdr2-interop.md). Both feed the same axis — extending
> to a new distro (jazzy/rolling) needs BOTH the type hash (here) and the
> encoding profile (there). Coordinated by
> [phase-304](phase-304-complete-ros-edition-axis.md) (axis completion + the
> multi-distro test method); phase-304 W1 drives this phase's RIHS01 work.

## Goal

Add RIHS01 type hash computation for ROS 2 Iron and later distros, enabling nros ↔ ROS 2 Iron+ interoperability.

## Background

ROS 2 Humble uses `TypeHashNotSupported` in data key expressions and a placeholder hash (`RIHS01_<64 zeros>`) in liveliness tokens. This works correctly for Humble interop and is the current nros behavior.

Starting with Iron, ROS 2 computes actual RIHS01 SHA-256 hashes per REP-2011. Without correct hashes, Iron+ nodes may reject nros messages or fail discovery.

## Current State

- **Humble interop**: Fully working (Phase 16 complete)
- Data keyexpr: `<domain>/<topic>/<type>/TypeHashNotSupported`
- Liveliness tokens: `RIHS01_<64 zeros>` placeholder
- Code generator placeholder at `packages/codegen/packages/rosidl-codegen/src/generator.rs`
  — a path from BEFORE codegen moved into `packages/cli/`; the file is now
  `packages/cli/rosidl-bindgen/src/generator.rs`. This section describes the
  state the phase STARTED from, so the old path is left standing as history.

## RIHS01 Format (REP-2011)

- Format: `RIHS01_<sha256_hex>` (64-character lowercase hex)
- SHA-256 computed from canonical type description in rosidl format
- Requires normalized text representation of message structure

## Implementation Options

1. **Extract from ament index** — Read hash files from installed ROS 2 packages at codegen time
2. **Compute in code generator** — Add `sha2` crate to `rosidl-codegen`, implement canonical format per REP-2011
3. **Hybrid** — Use ament index when available, compute otherwise

## Steps

### 41.1: Research REP-2011 canonical format

- [x] Research exact canonical type description format (REP-2011 normalization rules) — `docs/research/rep-2011-type-hash.md` (2026-05-17)
- [x] Document the normalization algorithm (field ordering, nested type expansion, bounded types) — same doc
- [x] Collect reference hashes from ROS 2 Iron/Jazzy for common types — the
      TODO values were resolved by an ORACLE rather than a transcription:
      `packages/cli/rosidl-bindgen/tests/edition_hash_oracle.rs` proves
      `engine == fixture == live Jazzy`, so the reference is re-derivable
      instead of copied once from a host nobody still has.

> **The 41.2-41.5 boxes below are TICKED against a mechanism that differs from
> the one they name, and the difference is the point.** They were written for a
> single `ros-iron` cargo FEATURE on `rosidl-codegen`; what shipped is an
> EDITION AXIS (`--ros-edition {humble,iron,jazzy,rolling}`, `RosEdition::parse`
> as the one parser, `ros-<edition>` passthrough features) — RFC-0056, driven by
> phase-304 W1. The box's intent is met and its spelling is not, so each says
> what actually satisfies it. Verified against `main` 2026-09-04.

### 41.2: Add `ros-iron` feature flag to code generator

- [x] Add `ros-iron` feature flag to `rosidl-codegen` — delivered as the EDITION
      ENUM, not a feature: `RosEdition` in `packages/cli/rosidl-codegen/src/types.rs`,
      covering humble/iron/jazzy/rolling.
- [x] Wire feature through `cargo-nano-ros` CLI — `nros generate-rust
      --ros-edition <e>`; `RosEdition::parse` is the single parser, reached from
      `nros-cli-core` (`cmd/ws.rs:2597`,
      `orchestration/cargo_metadata_schema.rs:1228`). `--ros-edition jazzy` used
      to be rejected because `cargo-nano-ros` kept a second, narrower parser that
      knew only humble|iron; routing it through the SSoT is what fixed it.
- [x] Ensure `ros-humble` remains the default behavior — humble still emits the
      `TypeHashNotSupported` keyexpr tail and the placeholder token
      (`nros-rmw-zenoh/src/keyexpr.rs:31,136`).

### 41.3: Implement RIHS01 hash computation

- [x] Add `sha2` crate dependency to `rosidl-codegen` — it landed in
      `packages/cli/rosidl-resolve` (`Cargo.toml:17`), which is where the hash
      engine went when codegen moved into `packages/cli/`.
- [x] Implement canonical type description serialization per REP-2011 —
      `rosidl-resolve/src/rihs.rs:366` `build_type_description`.
- [x] Compute SHA-256 and format as `RIHS01_<sha256_hex>` — `rihs.rs:773`
      `rihs01`, for message, service and action.
- [x] Emit computed hash in generated code — `rosidl-bindgen/src/generator.rs:122`;
      `std_msgs/Int32` under jazzy emits the real
      `RIHS01_b6578ded3c58c626cfe8d1a6fb6e04f706f97e9f03d2727c9ff4e74b1cef0deb`,
      humble still emits `TypeHashNotSupported` (both asserted at
      `generator.rs:1165,1177`).
- [x] Verify generated hashes match reference hashes from 41.1 — that IS the
      oracle test above, and it is the reason 41.1 could be closed at all.

### 41.4: Integrate hashes into RMW layer

- [x] Update data keyexpr to use computed hash when the edition supports it —
      `nros-rmw-zenoh/src/keyexpr.rs:14-45`.
- [x] Update liveliness tokens to use computed hash — same file; the token and
      the data keyexpr share the tail.
- [x] Ensure `ros-humble` path remains unchanged (`TypeHashNotSupported` /
      placeholder) — `keyexpr.rs:31` (topics) and `:136` (services).

### 41.5: Iron+ interop testing

- [x] Test against ROS 2 Iron (or Jazzy/Rolling) nodes — the zenoh lane runs a
      `ros-jazzy` zpico node against a stock jazzy `rmw_zenoh_cpp` over a shared
      `rmw_zenohd`.
- [x] Verify bidirectional pub/sub, services, and actions — six cells, both
      directions (`ros_editions_e2e.rs:153-158`).
- [x] Add interop tests, gated on the edition — NOT in `rmw_interop.rs`: issue
      #327 collapsed the five `ros_editions_*` files into one matrix rstest
      (`f74ffa5fb`), so the lane is `cargo test --test ros_editions_e2e zenoh`.
      The doc's earlier `ros_editions_zenoh.rs` is that file's predecessor.

## Acceptance Criteria

- Generated types include correct RIHS01 hash matching ROS 2 Iron's computation
- nros ↔ ROS 2 Iron bidirectional pub/sub, services, and actions work
- Humble behavior unchanged when `ros-humble` feature is active
