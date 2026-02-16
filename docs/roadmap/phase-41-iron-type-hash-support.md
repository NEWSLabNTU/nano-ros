# Phase 41: Iron+ Type Hash Support

**Status: Not Started**
**Priority: Low**
**Prerequisites:** Phase 16 (ROS 2 interop — complete for Humble)

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

- [ ] Research exact canonical type description format (REP-2011 normalization rules)
- [ ] Document the normalization algorithm (field ordering, nested type expansion, bounded types)
- [ ] Collect reference hashes from ROS 2 Iron/Jazzy for common types (`std_msgs/Int32`, `example_interfaces/AddTwoInts`, etc.)

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
