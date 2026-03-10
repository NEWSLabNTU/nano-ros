# Message Delivery Guarantees — System Design

> Date: 2026-03-10
> Status: Design proposal (not yet implemented)
> Depends on: E2E Safety Protocol Integration (`docs/design/e2e-safety-protocol-integration.md`)

## 1. Problem Statement

An RTOS node running nano-ros typically hosts multiple publishers and subscribers with radically different delivery requirements:

| Message class | Payload | Rate | Delivery requirement | Example |
|---|---|---|---|---|
| **Control/actuation** | 8–64 B | 100–1000 Hz | 99.9% delivery, bounded latency ≤ 5 ms | `/cmd_vel`, `/joint_commands` |
| **Sensor streams** | 256 B – 64 KB | 10–100 Hz | Best-effort, tolerate loss | `/imu/data`, `/lidar_scan` |
| **Bulk occasional** | 64 KB – 2 MB | On-demand | Complete delivery, latency secondary | `/map`, `/trajectory_plan` |

Today, nros treats all flows equally: same single-slot buffer, same spin-loop priority, same zenoh-pico fragmentation pool. A large sensor message being fragmented and reassembled can delay a small control message sharing the same TCP session. A bulk map transfer can exhaust the fragmentation buffer, causing unrelated small messages to fail.

The goal is to design a **message contract** system — a per-topic declaration of reliability, latency, and resource bounds — and show how nros, zenoh-pico, and the platform layer cooperate to honor those contracts on RTOS targets.

## 2. Current Architecture Gaps

### 2.1 Single-slot buffer — no depth control

Every subscriber uses exactly one static buffer. Zenoh-pico callbacks overwrite the previous value unconditionally. For control messages, this means a burst of 2 messages within one spin period loses one — unacceptable for 99.9% delivery targets. For sensor streams, single-slot is appropriate (latest-value semantics), but there is no way to opt in or out.

### 2.2 Flat spin loop — no priority differentiation

`spin_once()` iterates all callbacks in registration order. A long-running sensor deserialization (e.g., deserializing a 4 KB `LaserScan`) blocks subsequent control callbacks within the same spin cycle. No preemption, no priority ordering.

### 2.3 Shared fragmentation pool

zenoh-pico uses a single defragmentation buffer per session (`Z_FRAG_MAX_SIZE`). While one large message is being reassembled (possibly across multiple network reads), the buffer is occupied. If a second fragmented message arrives concurrently, it cannot begin reassembly — it is dropped. Small messages that fit in a single batch frame are unaffected, but any topic with payloads > `Z_BATCH_UNICAST_SIZE` competes for this single resource.

### 2.4 No admission control

There is no mechanism to reject a `create_publisher()` / `create_subscription()` call when the declared resource requirements exceed what the node can provide. Buffer exhaustion is discovered at runtime, not at setup time.

### 2.5 No deadline monitoring

The executor has no concept of "this callback must fire within X ms of data arrival." A missed control deadline is silent.

## 3. Message Contract

A **message contract** is a per-topic, compile-time declaration that bundles QoS with resource and timing commitments. It replaces the current `QosSettings` as the primary configuration surface.

```rust
/// Compile-time message contract.
pub struct MessageContract {
    // --- QoS (existing) ---
    pub reliability: Reliability,      // BestEffort | Reliable
    pub durability:  Durability,       // Volatile | TransientLocal

    // --- Delivery guarantee (new) ---
    pub delivery:    DeliveryClass,    // ControlCritical | SensorStream | BulkTransfer
    pub target_rate: Option<Hz>,       // Expected publish rate (for admission control)
    pub deadline_ms: Option<u32>,      // Max time from network arrival to callback start
    pub max_payload: usize,            // Worst-case serialized size (CDR, including header)

    // --- Buffer policy (new) ---
    pub history_depth: u16,            // 1 = single-slot (sensor), N = ring buffer (control)
    pub drop_policy:   DropPolicy,     // DropOldest | DropNewest | Block(timeout_ms)
}

pub enum DeliveryClass {
    /// Small, high-rate, high-reliability. Gets priority scheduling and
    /// dedicated non-fragmented path. Must fit in a single zenoh batch frame.
    ControlCritical,

    /// Medium-to-large, moderate rate, tolerates loss.
    /// Uses latest-value semantics (single-slot).
    SensorStream,

    /// Large, infrequent. Uses dedicated fragmentation resources.
    /// Published on a background session or separate zenoh channel.
    BulkTransfer,
}
```

### 3.1 Contract validation (setup time)

When a publisher or subscriber is created, the executor **validates** the contract against available resources:

```
fn validate_contract(contract: &MessageContract) -> Result<(), ContractError> {
    // 1. ControlCritical payloads must fit in one batch frame
    if contract.delivery == ControlCritical
        && contract.max_payload > Z_BATCH_UNICAST_SIZE - ZENOH_FRAME_OVERHEAD
    {
        return Err(PayloadTooLargeForControl);
    }

    // 2. Sum of all subscriber buffers must fit in available memory
    let required = contract.max_payload * contract.history_depth;
    if !memory_pool.can_allocate(required) {
        return Err(InsufficientMemory { required, available });
    }

    // 3. Deadline must be achievable given spin period
    if let Some(deadline) = contract.deadline_ms {
        if deadline < executor.worst_case_spin_period_ms() {
            return Err(DeadlineUnachievable { deadline, spin_period });
        }
    }

    Ok(())
}
```

This turns silent runtime buffer overflows into loud compile-time or setup-time errors.

## 4. Layered Guarantee Architecture

The guarantee is a cooperation across three layers. Each layer provides specific properties that the layer above relies on.

```
┌─────────────────────────────────────────────────────┐
│                    nros-node                        │
│  • Contract validation      • Deadline monitoring   │
│  • Priority-ordered spin    • Ring-buffer queues     │
│  • Admission control        • Integrity (safety-e2e) │
├─────────────────────────────────────────────────────┤
│                  nros-rmw-zenoh                      │
│  • Per-class session/channel separation             │
│  • Non-fragmented fast path for control messages    │
│  • Dedicated defrag buffer for bulk transfers       │
│  • Congestion-aware publish (try_publish semantics) │
├─────────────────────────────────────────────────────┤
│          Platform (FreeRTOS / Zephyr / NuttX)       │
│  • Priority-based task scheduling                   │
│  • Separate network task for bulk traffic           │
│  • Hardware timer for deadline watchdog             │
│  • Memory pool partitioning (MPU regions)           │
└─────────────────────────────────────────────────────┘
```

### 4.1 Platform layer responsibilities

| Mechanism | FreeRTOS | Zephyr | NuttX | Bare-metal |
|---|---|---|---|---|
| Priority spin task | `xTaskCreate` with `configMAX_PRIORITIES - 1` | `K_PRIO_COOP(0)` cooperative thread | `SCHED_FIFO` with high priority | ISR-driven or single loop |
| Bulk transfer task | Lower-priority task | `K_PRIO_PREEMPT(7)` preemptible | `SCHED_FIFO` lower priority | Deferred to idle |
| Deadline watchdog | `xTimerCreate` software timer | `k_timer` or hardware counter | POSIX timer or watchdog | Hardware timer ISR |
| Memory partitions | Static pools + `pvPortMalloc` regions | `k_mem_slab` / `k_heap` | `mm_initialize` regions | Linker-script sections |
| Network isolation | ISR + deferred processing task | Separate `net_mgmt` thread | Network worker thread | Poll in main loop |

**Key platform contract**: The platform must guarantee that a high-priority spin task preempts bulk transfer work. On bare-metal (single-loop), this degrades to cooperative scheduling within `spin_once()`.

### 4.2 RMW layer responsibilities (nros-rmw-zenoh)

#### 4.2.1 Session separation by traffic class

Instead of one zenoh-pico session per node, use up to three logical channels:

```
┌──────────────────────────┐
│     zenoh-pico session   │  ← Single TCP/UDP connection to zenohd
│                          │
│  Channel 0: CONTROL      │  Priority = 6 (zenoh RealTime)
│    - Never fragmented    │  Batch size: 256 B
│    - Dedicated send slot │
│                          │
│  Channel 1: SENSOR       │  Priority = 4 (zenoh Data)
│    - May fragment        │  Uses session's default defrag buffer
│    - Overwrites OK       │
│                          │
│  Channel 2: BULK         │  Priority = 2 (zenoh Background)
│    - Always fragments    │  Dedicated defrag buffer (large)
│    - Flow-controlled     │
└──────────────────────────┘
```

zenoh-pico supports priority levels in the transport protocol (the `Priority` field in frame headers). Today nros does not set it — all messages go at default priority. By mapping `DeliveryClass` to zenoh priority, the router can schedule control frames ahead of bulk fragments even when they share a TCP connection.

**Implementation path**: zenoh-pico's `z_publisher_put_options_t` has a `priority` field. nros-rmw-zenoh already creates `z_publisher_t` handles — adding `z_priority_t` assignment requires only a config change, no protocol modification.

#### 4.2.2 Non-fragmented fast path

For `ControlCritical` topics, the contract guarantees `max_payload ≤ Z_BATCH_UNICAST_SIZE - overhead`. This means:

- The message always fits in a single zenoh batch frame.
- No fragmentation/reassembly is needed — zero contention with the defrag buffer.
- Publish latency is deterministic: one `send()` syscall (or smoltcp write).

The RMW layer enforces this at `publish()` time:

```rust
fn publish(&self, payload: &[u8]) -> Result<(), TransportError> {
    if self.contract.delivery == ControlCritical {
        debug_assert!(payload.len() <= self.max_unfragmented_size);
        // Use zenoh put with PUSH reliability — single frame, no ack wait
    }
    // ...
}
```

#### 4.2.3 Dedicated defragmentation for bulk

For `BulkTransfer`, allocate a separate defrag buffer:

```
Current: 1 defrag buffer per session (Z_FRAG_MAX_SIZE = 2048 on embedded)
    → Bulk reassembly blocks sensor reassembly

Proposed: 2 defrag buffers
    Buffer 0 (2 KB): sensor traffic (shared with default)
    Buffer 1 (64 KB): bulk traffic only
```

zenoh-pico's `_z_transport_unicast_t` has a single `_z_wbuf_t _dbuf_reliable` and `_dbuf_best_effort`. To add a second defrag buffer, we either:

1. **Patch zenoh-pico** to support per-priority defrag buffers (preferred — aligns with zenoh protocol's priority model).
2. **Use a second session** for bulk traffic (simpler but doubles connection overhead).

Option 2 is pragmatic for a first implementation. The bulk session connects to the same router but uses a separate TCP socket, isolating its fragmentation state entirely.

### 4.3 nros-node layer responsibilities

#### 4.3.1 Ring-buffer subscriber queues

Replace the single-slot buffer with a configurable ring buffer for `ControlCritical` topics:

```
                    Single-slot (sensor)         Ring buffer (control, depth=4)
                    ┌─────────┐                  ┌───┬───┬───┬───┐
Callback writes →   │ latest  │                  │ 0 │ 1 │ 2 │ 3 │ ← head/tail
                    └─────────┘                  └───┴───┴───┴───┘
On spin:            Read once, clear flag        Read oldest, advance tail
Overflow:           Overwrite (OK for sensor)    DropOldest or reject (configurable)
```

Memory layout for `no_std`: the ring buffer is a fixed-size `[u8; max_payload * depth]` array, statically allocated. The zenoh-pico callback writes to `buffer[head * max_payload .. (head+1) * max_payload]` and advances head. The executor reads from tail.

**Synchronization**: head is written by the zenoh callback (ISR context on some platforms), tail is read/written by the executor (task context). Use atomic indices with acquire/release ordering — no mutex, no allocation.

```rust
struct RingSubscriberBuffer<const SIZE: usize, const DEPTH: usize> {
    data: [u8; SIZE * DEPTH],     // Static storage
    lengths: [AtomicU16; DEPTH],  // Payload length per slot (0 = empty)
    head: AtomicU16,              // Next write position (callback)
    tail: AtomicU16,              // Next read position (executor)
    overflow_count: AtomicU32,    // Diagnostic counter
}
```

For `SensorStream`, depth = 1 remains the default (latest-value semantics, zero overhead over today's implementation).

#### 4.3.2 Priority-ordered dispatch

Change the spin loop from registration-order to priority-order:

```
Current spin_once():
    for entry in arena.entries():   // registration order
        entry.try_process()

Proposed spin_once():
    // Phase 1: ControlCritical callbacks (deterministic, fast)
    for entry in arena.entries_by_class(ControlCritical):
        entry.try_process()

    // Phase 2: SensorStream callbacks
    for entry in arena.entries_by_class(SensorStream):
        entry.try_process()

    // Phase 3: BulkTransfer callbacks (if time remains)
    if !deadline_pressure() {
        for entry in arena.entries_by_class(BulkTransfer):
            entry.try_process()
    }
```

On RTOS platforms, the bulk phase can be skipped entirely when the spin period is running late, deferring bulk processing to the next cycle.

#### 4.3.3 Deadline monitoring

For topics with `deadline_ms` set, track time from data arrival to callback invocation:

```rust
// In zenoh callback (ISR/task context):
buffer.arrival_timestamp = platform::now_ticks();

// In spin loop, before dispatching:
let latency = platform::now_ticks() - buffer.arrival_timestamp;
if latency > entry.contract.deadline_ms * ticks_per_ms {
    entry.deadline_violations += 1;
    // Optionally: invoke a deadline-miss callback
    // Optionally: still process (data may be stale but useful)
}
```

The deadline counter is exposed via the parameter service (`~/deadline_violations`) and can be queried by a system monitor. On safety-critical deployments, a deadline miss triggers a user-registered fault handler.

#### 4.3.4 Admission control

At `Executor::add_subscription()` time, accumulate resource commitments:

```rust
struct ResourceBudget {
    total_buffer_bytes: usize,       // Sum of max_payload * depth for all subs
    max_buffer_bytes: usize,         // Platform memory limit
    control_slot_count: u8,          // ControlCritical subs (limited to N)
    max_control_slots: u8,           // Platform limit (e.g., 4)
    worst_case_spin_us: u32,         // Sum of worst-case callback durations
    spin_period_us: u32,             // Target spin period
}
```

If the budget is exceeded, `add_subscription()` returns `Err(ResourceExhausted)` at setup time rather than silently dropping messages at runtime.

## 5. Practical Scenarios

### 5.1 Scenario: Autonomous vehicle safety controller (FreeRTOS, Cortex-M7)

**Setup**: One node, 6 topics.

| Topic | Class | Payload | Rate | Depth | Buffer |
|---|---|---|---|---|---|
| `/cmd_vel` (pub) | ControlCritical | 24 B | 100 Hz | — | — |
| `/emergency_stop` (sub) | ControlCritical | 1 B | On-demand | 4 | 4 B |
| `/joint_state` (sub) | ControlCritical | 48 B | 200 Hz | 4 | 192 B |
| `/imu/data` (sub) | SensorStream | 128 B | 100 Hz | 1 | 128 B |
| `/lidar_scan` (sub) | SensorStream | 4 KB | 10 Hz | 1 | 4 KB |
| `/trajectory` (sub) | BulkTransfer | 128 KB | 0.5 Hz | 1 | 128 KB |

**Total subscriber buffer**: 4 + 192 + 128 + 4096 + 131,072 = **135,492 B** (~132 KB)

**Execution flow**:

```
FreeRTOS task layout:
  Task "control_spin" — Priority 5 (highest app-level)
    → spin_once() at 1 kHz (1 ms period)
    → Processes: emergency_stop, joint_state, cmd_vel publish
    → Processes: imu (if ready), lidar (if ready)
    → Skips: trajectory (deferred to bulk task)

  Task "bulk_recv" — Priority 2
    → Handles trajectory reassembly via second zenoh session
    → Copies completed trajectory to shared buffer
    → Sets guard condition to wake control_spin

  Task "network" — Priority 6 (above control)
    → ISR-driven: ethernet RX → smoltcp poll → zenoh-pico read
    → Writes directly to subscriber ring buffers
```

**Guarantee analysis**:

- **`/emergency_stop`**: 1-byte payload, never fragmented. Ring depth = 4 means up to 4 can queue between spin cycles. At 1 kHz spin, even if 3 arrive in a burst, none are lost. Deadline = 1 ms enforced by spin period.

- **`/joint_state`**: 48-byte payload at 200 Hz. At 1 kHz spin, at most 1 arrives per cycle — ring depth 4 absorbs jitter. 99.9% delivery achieved if network delivers within the ring's capacity (4 ms window).

- **`/lidar_scan`**: 4 KB payload, may fragment into 4–5 batch frames. Fragmentation uses the sensor defrag buffer. If a fragment is lost (best-effort), the entire message is dropped — acceptable for sensor streams.

- **`/trajectory`**: 128 KB, fragments into ~128 frames. Reassembled on the bulk session, completely isolated from the control path. The control task never blocks waiting for trajectory fragments.

### 5.2 Scenario: Multi-sensor fusion node (Zephyr, nRF5340)

**Setup**: Resource-constrained (256 KB RAM), 4 topics.

| Topic | Class | Payload | Rate | Depth | Buffer |
|---|---|---|---|---|---|
| `/motor_cmd` (pub) | ControlCritical | 16 B | 50 Hz | — | — |
| `/encoder` (sub) | ControlCritical | 12 B | 200 Hz | 8 | 96 B |
| `/temperature` (sub) | SensorStream | 8 B | 1 Hz | 1 | 8 B |
| `/firmware_update` (sub) | BulkTransfer | 64 KB | On-demand | 1 | 64 KB |

**Total subscriber buffer**: 96 + 8 + 65,536 = **65,640 B** (~64 KB, 25% of RAM)

**Admission control** rejects adding a second BulkTransfer subscription (would exceed RAM budget). The system designer knows this at `main()` init, not at 3 AM in production.

**Zephyr-specific**:

```c
// Platform provides k_mem_slab for deterministic allocation
K_MEM_SLAB_DEFINE(control_slab, 96, 1, 4);   // 96 B, 1 block
K_MEM_SLAB_DEFINE(bulk_slab, 65536, 1, 4);   // 64 KB, 1 block

// Spin thread at cooperative priority
K_THREAD_DEFINE(spin_tid, 2048, spin_entry, NULL, NULL, NULL,
                K_PRIO_COOP(1), 0, 0);

// Bulk receive at preemptible priority
K_THREAD_DEFINE(bulk_tid, 4096, bulk_entry, NULL, NULL, NULL,
                K_PRIO_PREEMPT(7), 0, 0);
```

### 5.3 Scenario: Sensor gateway (bare-metal, Cortex-M3)

No RTOS, single main loop. Priority differentiation is achieved cooperatively:

```rust
loop {
    // High-priority: poll network + process control callbacks
    session.spin_once(0);  // non-blocking
    executor.dispatch_class(ControlCritical);

    // Medium-priority: sensor callbacks (every other iteration)
    if loop_count % 2 == 0 {
        executor.dispatch_class(SensorStream);
    }

    // Low-priority: bulk callbacks (every 100th iteration)
    if loop_count % 100 == 0 {
        executor.dispatch_class(BulkTransfer);
    }

    platform::sleep_until_next_tick();
}
```

This is less deterministic than RTOS-based preemption, but it still ensures control messages are processed on every iteration while bulk transfers only consume cycles when there is slack.

## 6. Fragmentation Budget Model

A key insight: **control messages should never fragment**. This must be enforced by the contract system.

### 6.1 zenoh-pico frame overhead

```
Zenoh batch frame layout:
  ┌────────────┬───────────────────────────────────────┐
  │ Frame hdr  │ Payload                               │
  │ ~16 bytes  │ (up to Z_BATCH_UNICAST_SIZE - 16)     │
  └────────────┴───────────────────────────────────────┘

  Frame header includes:
    - Transport header (2 B)
    - Channel + reliability (1 B)
    - SN (VLE, 1-5 B)
    - Priority (1 B)
    - Key expression (VLE length + bytes, typically 30–60 B for ROS topics)
    - Encoding (1 B)
    - Attachment length (VLE, 1-2 B)
    - Attachment data (33–37 B for RMW attachment)
```

Conservative overhead estimate: **120 bytes** per frame (topic keyexpr + attachment + headers).

With `Z_BATCH_UNICAST_SIZE = 1024` (embedded default), maximum unfragmented payload = **~900 bytes**. For control messages (typically 8–64 B), this is more than sufficient.

### 6.2 Fragmentation resource allocation

| DeliveryClass | Defrag buffer | Source | Sizing |
|---|---|---|---|
| ControlCritical | None (never fragments) | — | — |
| SensorStream | Session default | `Z_FRAG_MAX_SIZE` | `max(max_payload)` across all sensor subs |
| BulkTransfer | Dedicated (second session or patched defrag) | Separate allocation | `max(max_payload)` across all bulk subs |

The contract's `max_payload` field feeds directly into `Z_FRAG_MAX_SIZE` configuration at build time, ensuring the defrag buffer is always large enough.

## 7. Interaction with safety-e2e

The existing safety-e2e feature (CRC-32, sequence tracking, `IntegrityStatus`) integrates naturally with message contracts:

| Contract field | safety-e2e interaction |
|---|---|
| `deadline_ms` | If set, safety-e2e adds a freshness check: reject messages whose timestamp is older than `deadline_ms` |
| `history_depth > 1` | Sequence gap detection accounts for ring buffer depth — a gap of ≤ depth is normal (consumed), > depth is a real loss |
| `ControlCritical` | Enables stricter mode: any sequence gap or CRC failure triggers a fault callback rather than a silent counter increment |
| `target_rate` | Enables timeout-based loss detection: if no message arrives within `2 / target_rate`, declare a source-loss fault |

## 8. Contract Specification Format

For static (`no_std`) systems, contracts are compile-time constants:

```rust
const CMD_VEL_CONTRACT: MessageContract = MessageContract {
    reliability: Reliability::Reliable,
    durability: Durability::Volatile,
    delivery: DeliveryClass::ControlCritical,
    target_rate: Some(Hz(100)),
    deadline_ms: Some(5),
    max_payload: 24,      // geometry_msgs/Twist CDR size
    history_depth: 4,
    drop_policy: DropPolicy::DropOldest,
};

// Used at subscription creation:
executor.add_subscription_with_contract::<Twist>(
    "/cmd_vel",
    &CMD_VEL_CONTRACT,
    |msg| { /* control logic */ },
)?;
```

For `std` systems, contracts can also be loaded from a YAML/TOML manifest:

```toml
[[topics]]
name = "/cmd_vel"
type = "geometry_msgs/msg/Twist"
delivery = "control_critical"
target_rate_hz = 100
deadline_ms = 5
max_payload = 24
history_depth = 4
drop_policy = "drop_oldest"
```

## 9. Diagnostics and Observability

Each contract-bound subscription exposes runtime counters via the parameter service:

| Parameter | Type | Description |
|---|---|---|
| `~/contracts/<topic>/messages_received` | u64 | Total messages delivered to callback |
| `~/contracts/<topic>/messages_dropped` | u64 | Messages dropped (buffer full + drop policy) |
| `~/contracts/<topic>/deadline_violations` | u64 | Callbacks that exceeded `deadline_ms` |
| `~/contracts/<topic>/sequence_gaps` | u64 | Detected sequence discontinuities |
| `~/contracts/<topic>/crc_failures` | u64 | CRC mismatches (safety-e2e only) |
| `~/contracts/<topic>/max_latency_us` | u64 | High-water mark: arrival → callback |
| `~/contracts/<topic>/buffer_utilization` | f32 | Ring buffer fill level (0.0–1.0) |

On systems without the parameter service (bare-metal), these counters are accessible via a `ContractStats` struct returned by `executor.contract_stats(topic)`.

## 10. Implementation Phases

| Phase | Scope | Dependencies |
|---|---|---|
| **A** | `MessageContract` struct + admission control at `add_subscription()` | nros-node only |
| **B** | Ring-buffer subscriber queue (depth > 1) with atomic head/tail | nros-rmw-zenoh shim |
| **C** | Priority-ordered dispatch in spin loop | nros-node executor |
| **D** | zenoh priority assignment per `DeliveryClass` | nros-rmw-zenoh publisher |
| **E** | Deadline monitoring + fault callbacks | nros-node + platform clock |
| **F** | Bulk transfer session isolation (second zenoh session) | nros-rmw-zenoh session |
| **G** | Diagnostics via parameter service | nros-node + nros-params |
| **H** | safety-e2e integration (freshness, strict mode) | nros-node safety-e2e feature |

Phases A–C are purely within nros and require no zenoh-pico patches. Phase D uses existing zenoh-pico API. Phase F may require a zenoh-pico patch for per-priority defrag or can use the two-session workaround.

## 11. Open Questions

1. **Ring buffer lock-freedom on Cortex-M0** — Cortex-M0 lacks `LDREX`/`STREX`. The ring buffer's atomic head/tail requires CAS or equivalent. Fallback: disable interrupts during head advance (acceptable for the ~10-cycle critical section).

2. **zenoh-pico priority support maturity** — The `z_priority_t` field exists in the API but its effect on router scheduling depends on zenohd version. Need to verify behavior with zenoh 1.6.2.

3. **Contract inheritance for actions** — An action is composed of goal (service), feedback (topic), and result (service). Should the action inherit a single contract or allow per-channel contracts?

4. **Contract compatibility checking** — When a `ControlCritical` publisher connects to a `SensorStream` subscriber (or vice versa), should the system warn? The zenoh layer doesn't enforce this — it would need to be an application-level convention.

5. **Dynamic contract renegotiation** — On systems with `alloc`, should contracts be adjustable at runtime (e.g., switching a topic from `SensorStream` to `ControlCritical` during a mode change)? This conflicts with the static-allocation principle but may be needed for multi-mode systems.
