# Phase 41: Iron+ Type Hash Support

**Status: Substantially complete (2026-07-26).** The RIHS01 machinery is built,
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
check now exists: `packages/testing/nros-tests/tests/ros_editions_zenoh.rs` runs a
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
- [ ] Collect reference hashes from ROS 2 Iron/Jazzy for common types — canonical inputs for `std_msgs/msg/Int32` + `example_interfaces/srv/AddTwoInts` derived in the research doc, but the `RIHS01_...` values are flagged TODO; `/opt/ros/humble` predates REP-2011 (`ros2 interface hash` is Iron+). Verify on a Jazzy host before 41.3 lands a fixture.

### 41.2: Add `ros-iron` feature flag to code generator

- [ ] Add `ros-iron` feature flag to `rosidl-codegen`
- [ ] Wire feature through `cargo-nano-ros` CLI
- [ ] Ensure `ros-humble` remains the default behavior

### 41.3: Implement RIHS01 hash computation

- [ ] Add `sha2` crate dependency to `rosidl-codegen`
- [ ] Implement canonical type description serialization per REP-2011
- [ ] Compute SHA-256 and format as `RIHS01_<sha256_hex>`
- [ ] Emit computed hash in generated code (e.g., `const TYPE_HASH: &str`)
- [ ] Verify generated hashes match reference hashes from 41.1

### 41.4: Integrate hashes into RMW layer

- [ ] Update data keyexpr to use computed hash when `ros-iron` feature is active
- [ ] Update liveliness tokens to use computed hash
- [ ] Ensure `ros-humble` path remains unchanged (`TypeHashNotSupported` / placeholder)

### 41.5: Iron+ interop testing

- [ ] Test against ROS 2 Iron (or Jazzy/Rolling) nodes
- [ ] Verify bidirectional pub/sub, services, and actions
- [ ] Add interop tests to `rmw_interop.rs` gated on `ros-iron` feature

## Acceptance Criteria

- Generated types include correct RIHS01 hash matching ROS 2 Iron's computation
- nros ↔ ROS 2 Iron bidirectional pub/sub, services, and actions work
- Humble behavior unchanged when `ros-humble` feature is active
