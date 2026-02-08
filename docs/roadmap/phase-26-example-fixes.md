# Phase 26: Example Fixes — Typed Messages and Domain ID Isolation

**Goal**: (1) Migrate all examples from raw-byte message handling to generated typed bindings. (2) Add `ROS_DOMAIN_ID` support to all BSPs and examples so concurrent interop tests (QEMU ↔ native, Zephyr ↔ native, etc.) can share a single zenohd without topic collisions.

**Status**: Not Started
**Priority**: High
**Depends on**: None (all changes are to existing code)

## Problem Statement

### Raw bytes in examples

Many examples bypass the message type system entirely:

| Category | Examples | Issue |
|----------|----------|-------|
| **QEMU bare-metal** (4) | rs-talker, rs-listener, bsp-talker, bsp-listener | Publish/receive raw UTF-8 text — no CDR encoding, raw zenoh keyexpr `demo/qemu` (not ROS 2 compatible) |
| **STM32F4** (1) | bsp-talker | Inline manual CDR (`cdr_buffer[0..4] = header`) with hardcoded full keyexpr `b"0/chatter/std_msgs::msg::dds_::Int32_/TypeHashNotSupported\0"` |
| **Native C** (2) | c-talker, c-listener | Hand-written `std_msgs_Int32_serialize()`/`deserialize()` with `nano_ros_publish_raw()` |
| **Native C** (1) | c-baremetal-demo | Same manual CDR as c-talker |
| **Zephyr C** (2) | c-talker, c-listener | Hand-written CDR ser/de, but BSP builds proper keyexpr |

The QEMU examples are the worst — they aren't ROS 2 interoperable at all. The C examples at least construct valid CDR and keyexprs, but duplicate serialization logic that should come from generated bindings.

### No topic-level test isolation

All topic names are hardcoded (`/chatter`, `/add_two_ints`, `/fibonacci`). Tests currently rely on separate zenohd instances (ephemeral ports) for isolation, which works for native-to-native tests but is heavyweight for cross-architecture interop tests (QEMU, Zephyr) that need Docker/TAP networking to a shared zenohd.

`ROS_DOMAIN_ID` is already part of every data keyexpr (`<domain_id>/chatter/...`), so tests using different domain IDs are fully isolated even on a shared router. But only the native Rust/C examples and the Zephyr BSP currently support it. The QEMU and STM32F4 BSPs pass raw topic bytes with no domain ID handling.

## 26.1: Add `domain_id` to QEMU and STM32F4 BSPs

**Status**: Not Started

The Zephyr BSP already has domain ID support (`nano_ros_bsp_build_keyexpr()` prepends `<domain_id>` to topic/type). The QEMU and STM32F4 BSPs need the same.

### Current state

| BSP | Has `domain_id`? | Keyexpr construction | Topic API |
|-----|:-:|---|---|
| Zephyr | Yes (Kconfig `CONFIG_NANO_ROS_DOMAIN_ID`, `nano_ros_node_t.domain_id`) | `nano_ros_bsp_build_keyexpr()` builds `<domain>/<topic>/<type>/TypeHashNotSupported` | `create_publisher(&node, &pub, "/chatter", "std_msgs::msg::dds_::Int32_")` |
| QEMU | **No** | User passes raw `&[u8]` bytes directly to `zenoh_shim_declare_publisher()` | `node.create_publisher(b"demo/qemu\0")` — raw zenoh keyexpr |
| STM32F4 | **No** | Same as QEMU — raw `&[u8]` | `node.create_publisher(b"0/chatter/std_msgs::msg::dds_::Int32_/TypeHashNotSupported\0")` |

### Changes

**QEMU BSP (`crates/nano-ros-bsp-qemu/`)**:

1. [ ] Add `domain_id: u32` to `Config` (default 0)
2. [ ] Add builder `Config::with_domain_id(self, domain_id: u32) -> Self`
3. [ ] Add `create_ros_publisher(&mut self, topic: &str, type_name: &str) -> Result<Publisher>` to `Node`:
   - Constructs the keyexpr `<domain_id>/<topic_stripped>/<type_name>/TypeHashNotSupported` using a `heapless::String<256>` + null terminator
   - Calls existing `create_publisher()` with the constructed bytes
   - Keep existing raw `create_publisher(&[u8])` for backward compatibility
4. [ ] Add `create_ros_subscriber(...)` with same keyexpr construction + wildcard (`*` instead of `TypeHashNotSupported`)
5. [ ] Pass `domain_id` through `InnerNode::new()` and store it on the node

**STM32F4 BSP (`crates/nano-ros-bsp-stm32f4/`)**:

6. [ ] Add `domain_id: u32` to `Config` (default 0)
7. [ ] Add `create_ros_publisher()` / `create_ros_subscriber()` to `Node` (same pattern as QEMU)

**Helper**: Since both BSPs need the same keyexpr formatting, consider a shared `no_std` function:
```rust
/// Format a ROS 2 data keyexpr into a fixed buffer.
/// Returns the null-terminated slice, or None if the buffer is too small.
fn format_ros2_keyexpr<const N: usize>(
    buf: &mut heapless::String<N>,
    domain_id: u32,
    topic: &str,
    type_name: &str,
) {
    use core::fmt::Write;
    let topic_stripped = topic.trim_matches('/');
    write!(buf, "{}/{}/{}/TypeHashNotSupported", domain_id, topic_stripped, type_name).ok();
}
```

This duplicates `TopicInfo::to_key()` from `nano-ros-transport`, but the BSP crates cannot depend on `nano-ros-transport` (it pulls in `std` + zenoh). The duplication is acceptable for a 4-line function.

**Acceptance Criteria**:
- [ ] `Config::default()` has `domain_id: 0` (no breaking change)
- [ ] `create_ros_publisher("/chatter", "std_msgs::msg::dds_::Int32_")` on domain 0 produces keyexpr `0/chatter/std_msgs::msg::dds_::Int32_/TypeHashNotSupported`
- [ ] Existing raw `create_publisher(b"...")` still works (backward compat)

## 26.2: Migrate QEMU examples to typed messages and ROS 2 topics

**Status**: Not Started
**Depends on**: 26.1

The 4 QEMU examples currently publish/receive raw text over a non-ROS keyexpr `demo/qemu`. Migrate them to use CDR-encoded `std_msgs::msg::Int32` over a proper ROS 2 topic.

### Changes

**`examples/qemu/rs-talker/src/main.rs`** and **`examples/qemu/bsp-talker/src/main.rs`**:

1. [ ] Replace `b"demo/qemu\0"` with `node.create_ros_publisher("/chatter", "std_msgs::msg::dds_::Int32_")`
2. [ ] Replace raw text publishing with CDR-encoded Int32:
   ```rust
   // Before:
   publisher.publish(b"Hello from QEMU #5")

   // After:
   let mut buf = [0u8; 8];
   buf[0..4].copy_from_slice(&[0x00, 0x01, 0x00, 0x00]); // CDR header (LE)
   buf[4..8].copy_from_slice(&counter.to_le_bytes());       // Int32 payload
   publisher.publish(&buf)
   ```
   Note: These are `no_std` — they cannot use `CdrWriter` (requires `alloc`) or generated message types. Manual CDR for a single `i32` is 8 bytes and acceptable for `no_std` examples. A `// CDR: ...` comment explains the layout.

**`examples/qemu/rs-listener/src/main.rs`** and **`examples/qemu/bsp-listener/src/main.rs`**:

3. [ ] Replace `b"demo/qemu\0"` with `node.create_ros_subscriber("/chatter", "std_msgs::msg::dds_::Int32_")`
4. [ ] Replace raw text interpretation with CDR Int32 decoding:
   ```rust
   // In callback: data is *const u8, len is usize
   if len >= 8 {
       let value = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
       hprintln!("Received: {}", value);
   }
   ```

**Acceptance Criteria**:
- [ ] QEMU talker/listener interop with `native/rs-listener` / `native/rs-talker` via zenohd (CDR Int32 on `/chatter`)
- [ ] QEMU talker/listener interop with ROS 2 `ros2 topic echo /chatter std_msgs/msg/Int32` (via rmw_zenoh)
- [ ] `just test-qemu` still passes
- [ ] `bsp-talker` ↔ `bsp-listener` interop works within QEMU

## 26.3: Migrate STM32F4 example to BSP topic API

**Status**: Not Started
**Depends on**: 26.1

**`examples/stm32f4/bsp-talker/src/main.rs`**:

1. [ ] Replace hardcoded keyexpr `b"0/chatter/std_msgs::msg::dds_::Int32_/TypeHashNotSupported\0"` with `node.create_ros_publisher("/chatter", "std_msgs::msg::dds_::Int32_")`
2. [ ] Keep existing CDR encoding (already correct: 4-byte header + LE i32)

**Acceptance Criteria**:
- [ ] `stm32f4/bsp-talker` builds and interops with native listener (same CDR format as before)
- [ ] Domain ID comes from `Config` instead of being baked into the topic string

## 26.4: Migrate native C examples to generated bindings

**Status**: Not Started

The native C examples (`c-talker`, `c-listener`, `c-baremetal-demo`) hand-write CDR serialization. The `c-custom-msg` example already demonstrates using `cargo nano-ros generate` for typed C bindings. Migrate the others.

### Current state

`c-talker` and `c-listener` define hand-written functions:
```c
// Hand-written in c-talker:
int std_msgs_Int32_serialize(int32_t value, uint8_t* buffer, size_t buffer_size) {
    buffer[0] = 0x00; buffer[1] = 0x01; buffer[2] = 0x00; buffer[3] = 0x00;
    memcpy(&buffer[4], &value, sizeof(value));
    return 8;
}
```

The C API already provides `nano_ros_publish_raw()`. There is no `nano_ros_publish_typed()` that calls serialization automatically. So even with generated bindings, the C example would still call `_serialize()` then `publish_raw()`. But the serialization code comes from generation instead of being hand-written.

### Changes

**`examples/native/c-talker/`**:

1. [ ] Add `package.xml` with `std_msgs` dependency (or reuse the existing type support struct which already references `std_msgs::msg::dds_::Int32_`)
2. [ ] Run `cargo nano-ros generate` to produce `std_msgs__msg__Int32_serialize()` / `_deserialize()`
3. [ ] Replace hand-written `std_msgs_Int32_serialize()` with generated `std_msgs__msg__Int32_serialize()`
4. [ ] Use the generated `std_msgs__msg__Int32` struct for the message

**`examples/native/c-listener/`**:

5. [ ] Same: replace hand-written deserialize with generated code
6. [ ] Callback uses typed `std_msgs__msg__Int32` struct instead of raw byte pointer

**`examples/native/c-baremetal-demo/`**:

7. [ ] Same migration pattern as c-talker

**Acceptance Criteria**:
- [ ] No hand-written CDR serialization in any native C example
- [ ] `just test-c` passes
- [ ] C talker ↔ Rust listener interop still works

### Open question

The C message generation (`cargo nano-ros generate`) currently requires a ROS 2 environment for message definitions. For `std_msgs/Int32` this is straightforward, but it adds a build dependency. An alternative is to check in the generated headers (like micro-ROS does). Decision: defer to Phase 23 which bundles pre-generated headers.

## 26.5: Migrate Zephyr C examples to generated bindings

**Status**: Not Started

The Zephyr C examples (`c-talker`, `c-listener`) have hand-written CDR ser/de but already use the BSP's `nano_ros_bsp_create_publisher()` which constructs proper keyexprs with domain ID. Only the message serialization needs fixing.

### Changes

**`examples/zephyr/c-talker/src/main.c`**:

1. [ ] Replace hand-written `std_msgs_Int32_serialize()` with generated binding
2. [ ] Use typed `std_msgs__msg__Int32` struct

**`examples/zephyr/c-listener/src/main.c`**:

3. [ ] Replace hand-written `std_msgs_Int32_deserialize()` with generated binding
4. [ ] Callback uses typed struct

**Acceptance Criteria**:
- [ ] `just test-zephyr` passes
- [ ] Zephyr C talker ↔ Rust listener interop works

## 26.6: Domain ID in integration tests

**Status**: Not Started
**Depends on**: 26.1, 26.2

Enable concurrent cross-architecture interop tests by using unique `ROS_DOMAIN_ID` values instead of (or in addition to) separate zenohd instances.

### Current isolation model

```
Test A:  zenohd:54321  ←→  rs-talker (ZENOH_LOCATOR=tcp/127.0.0.1:54321)
Test B:  zenohd:54322  ←→  rs-talker (ZENOH_LOCATOR=tcp/127.0.0.1:54322)
                          ^^ Separate router per test = isolation
```

This works for native-only tests but is impractical for QEMU/Zephyr interop where the emulated node connects to a fixed locator via Docker/TAP networking.

### New isolation model

```
Shared zenohd:7447  ←→  QEMU talker (domain_id=100)  ←→  native listener (ROS_DOMAIN_ID=100)
                    ←→  QEMU talker (domain_id=101)  ←→  native listener (ROS_DOMAIN_ID=101)
                          ^^ Same router, different domain IDs = isolation
```

### Changes

1. [ ] Add a `unique_domain_id()` test helper in `crates/nano-ros-tests/src/`:
   ```rust
   use std::sync::atomic::{AtomicU32, Ordering};
   static DOMAIN_COUNTER: AtomicU32 = AtomicU32::new(100); // Start above typical manual usage

   pub fn unique_domain_id() -> u32 {
       DOMAIN_COUNTER.fetch_add(1, Ordering::Relaxed)
   }
   ```
2. [ ] For native example processes: set `ROS_DOMAIN_ID=<unique>` env var (already works)
3. [ ] For QEMU examples: pass domain ID as a compile-time constant or via BSP `Config`:
   - Option A: Build each test variant with a different `--cfg domain_id=N` (heavyweight — requires recompilation)
   - Option B: Pass domain ID via QEMU semihosting (read from a host file or env)
   - Option C: Use a fixed domain ID per test suite (e.g., emulator tests always use domain 50, zephyr tests use domain 60) — simplest, sufficient if test suites don't run the same topic in parallel internally
   - **Recommended**: Option C for now. Each test suite gets a reserved domain ID range. Within a suite, tests are already serialized (nextest `max-threads = 1` for emulator/zephyr groups).
4. [ ] For Zephyr examples: set `CONFIG_NANO_ROS_DOMAIN_ID` in `prj.conf` per test, or pass via Kconfig overlay
5. [ ] Document domain ID allocation in test docs:
   ```
   Domain ID ranges:
     0       — manual testing / examples
     1-49    — reserved
     50-59   — QEMU emulator tests
     60-69   — Zephyr tests
     100+    — native integration tests (auto-assigned by unique_domain_id())
   ```

**Acceptance Criteria**:
- [ ] Cross-architecture interop tests (QEMU ↔ native) can run concurrently with native-only tests
- [ ] No topic collisions between test suites sharing a zenohd

## Dependencies

```
26.1 (BSP domain_id) ──────┬───────────────────────────────────┐
         │                  │                                   │
         ▼                  ▼                                   ▼
26.2 (QEMU typed msgs)  26.3 (STM32F4 keyexpr)         26.6 (Test domain IDs)
                                                                │
26.4 (Native C gen)                                             │
                                                                │
26.5 (Zephyr C gen)                                             │
                                                                ▼
                                                    Concurrent interop tests
```

- **26.1** is the foundation — BSP domain ID support unlocks both example fixes and test isolation.
- **26.2, 26.3** depend on 26.1 (they use `create_ros_publisher()`).
- **26.4, 26.5** are independent of 26.1 (they only change serialization, not topic construction).
- **26.6** depends on 26.1 and 26.2 (needs domain ID in BSPs and ROS 2-compatible keyexprs in QEMU examples).

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| QEMU `no_std` examples can't use `CdrWriter`/generated types | Medium | Manual CDR for Int32 is trivial (8 bytes). Document the layout. Consider a `no_std` `cdr_encode_i32()` helper if more types are needed. |
| C message generation requires ROS 2 environment | Medium | For Int32, the CDR format is trivial. For Phase 23, pre-generate and bundle headers. |
| `heapless::String<256>` too small for long type names | Low | 256 bytes handles all standard ROS 2 types. Add a compile-time assertion. |
| Domain ID approach doesn't isolate within a test suite | Low | Test suites using QEMU/Zephyr already run with `max-threads = 1`. Domain ID isolates between suites. |
| Breaking change to BSP `Config` struct | Low | Adding `domain_id: u32` with default 0 is backward-compatible if users construct via `Config::default()` or builders. Direct struct literal construction breaks — document in changelog. |
