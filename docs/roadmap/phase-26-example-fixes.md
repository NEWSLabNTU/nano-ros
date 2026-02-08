# Phase 26: Typed BSP API and Example Migration

**Goal**: Replace raw-byte / string-based BSP APIs with rclrs-style typed generics (`Publisher<M>`, `Subscription<M>`) and migrate all examples to use them.

**Status**: In Progress
**Priority**: High
**Depends on**: None (all changes are to existing code)

## Problem Statement

### Previous API

The QEMU and STM32F4 BSP crates exposed two API levels, neither following rclrs conventions:

1. **Raw zenoh**: `create_publisher(topic: &[u8])` — user passes raw keyexpr bytes, leaks transport details
2. **String-based ROS**: `create_ros_publisher(topic: &str, type_name: &str)` — type name as string, bypasses compile-time type safety

Examples manually constructed CDR bytes (`buf[1] = 0x01; buf[4..8].copy_from_slice(...)`) and used unsafe `extern "C"` callbacks with raw pointers.

### New API

BSP crates now expose only rclrs-style generic methods:

```rust
// Publisher — typed generic, automatic CDR serialization
let pub_ = node.create_publisher::<Int32>("/chatter")?;
pub_.publish(&Int32 { data: 42 })?;

// Subscription — typed callback, automatic CDR deserialization
fn on_message(msg: &Int32) { /* ... */ }
let _sub = node.create_subscription::<Int32>("/chatter", on_message)?;
```

## 26.1: Typed BSP API revision

**Status**: Complete

### Changes

**Both BSP crates** (`nano-ros-bsp-qemu`, `nano-ros-bsp-stm32f4`):

- [x] Added `nano-ros-core` dependency (provides `RosMessage`, `CdrWriter`, `CdrReader`, `Serialize`, `Deserialize`)
- [x] `Publisher` → `Publisher<M: RosMessage>` with typed `publish(&M)` + CDR auto-serialization
- [x] `Subscriber` → `Subscription<M: RosMessage>` (rclrs naming)
- [x] Added CDR trampoline callback: `subscription_trampoline::<M>()` deserializes CDR and calls user's `fn(&M)`
- [x] Removed from public API: `create_publisher(&[u8])`, `create_ros_publisher(&str, &str)`, `create_subscriber(...)`, `create_ros_subscriber(...)`, `ShimCallback`
- [x] Added to public API: `create_publisher::<M>(&str)`, `create_subscription::<M>(&str, fn(&M))`
- [x] Updated prelude: removed `ShimCallback`/`Subscriber`, added `RosMessage`/`Serialize`/`Deserialize`/`Subscription`
- [x] Added error variants: `BufferTooSmall`, `Serialize`
- [x] Updated crate-level and method-level doc examples

### Architecture

```
User code                BSP crate                     nano-ros-core
─────────                ─────────                     ─────────────
Int32 { data: 42 }  →  Publisher<Int32>::publish()  →  CdrWriter::new_with_header()
                        ├── M::serialize(&writer)      ├── write CDR header
                        └── publish_raw(bytes)         └── write_i32(42)

CDR bytes            →  subscription_trampoline::<M>() → CdrReader::new_with_header()
                        ├── M::deserialize(&reader)      ├── skip CDR header
                        └── callback(&msg)               └── read_i32() → 42
```

### Dependency diagram

```
examples/qemu/*  ──→  nano-ros-bsp-qemu  ──→  nano-ros-core (RosMessage, CDR)
examples/stm32f4/* → nano-ros-bsp-stm32f4 →  nano-ros-core (RosMessage, CDR)
                                               └── nano-ros-serdes (no_std)
```

## 26.2: Migrate QEMU examples to typed API

**Status**: Complete

Updated 4 QEMU examples:
- [x] `examples/qemu/rs-talker/src/main.rs` — typed `Publisher<Int32>`, auto CDR
- [x] `examples/qemu/rs-listener/src/main.rs` — typed `Subscription<Int32>`, auto CDR deserialization
- [x] `examples/qemu/bsp-talker/src/main.rs` — same
- [x] `examples/qemu/bsp-listener/src/main.rs` — same

Each example defines a local `mod msg` with an `Int32` type implementing `Serialize`, `Deserialize`, and `RosMessage`.

## 26.3: Migrate STM32F4 example to typed API

**Status**: Complete

- [x] `examples/stm32f4/bsp-talker/src/main.rs` — typed `Publisher<Int32>`, auto CDR

## 26.4: Migrate native C examples to generated bindings

**Status**: Not Started

The native C examples (`c-talker`, `c-listener`, `c-baremetal-demo`) hand-write CDR serialization. Migration to generated C bindings is deferred to Phase 23 (Arduino precompiled library).

## 26.5: Migrate Zephyr C examples to generated bindings

**Status**: Not Started

The Zephyr C examples (`c-talker`, `c-listener`) have hand-written CDR ser/de. Migration deferred to Phase 23.

## 26.6: Domain ID in integration tests

**Status**: Not Started

Enable concurrent cross-architecture interop tests by using unique `ROS_DOMAIN_ID` values. Domain ID support is already in both BSP `Config` structs (added in earlier work). Remaining: test infrastructure changes.

## Not Changed

- `nano-ros-core` — already has `RosMessage`, `CdrWriter`, `CdrReader`
- `nano-ros-transport` — `TopicInfo` stays here (BSPs keep their local keyexpr formatting)
- `zenoh-pico-shim` / `zenoh-pico-shim-sys` — unchanged
- `nano-ros-bsp-zephyr` — C library, out of scope for this phase
