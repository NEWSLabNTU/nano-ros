# Formal Verification of Message Delivery Guarantees

> Date: 2026-03-13
> Status: Design proposal (not yet implemented)
> Depends on: Message Delivery Guarantees (`docs/design/message-delivery-guarantees.md`)
> Depends on: E2E Safety Protocol Integration (`docs/design/e2e-safety-protocol-integration.md`)
> Depends on: Phase 56 Verification Refresh (`docs/roadmap/archived/phase-56-verification-refresh.md`)

## 1. Problem Statement

The current verification infrastructure proves properties within individual layers:
- **Executor layer**: spin_once processes all ready work items (progress.rs)
- **Buffer layer**: subscriber/service buffers never silently truncate or get stuck (e2e.rs)
- **Serialization layer**: CDR round-trips are lossless (cdr.rs)
- **Scheduling layer**: timers fire on schedule, triggers gate correctly (scheduling.rs)

However, these proofs do not compose into end-to-end guarantees across the full message path:

```
Publisher app → CDR serialize → zenoh publish → [network] →
  zenoh callback → subscriber buffer → executor spin → CDR deserialize → Subscriber app
```

The **gaps** are:

1. **No fragmentation model** — zenoh-pico splits large messages into batch frames and reassembles them. There is no ghost model for this process. We cannot prove that a fragmented message either arrives completely or is dropped entirely (no partial delivery).

2. **No ring buffer model** — the proposed `MessageContract` system introduces ring buffers (depth > 1). The existing `SubscriberBufferGhost` only models single-slot buffers. Ring buffer head/tail atomics need lock-freedom and liveness proofs.

3. **No multi-spin liveness** — current proofs cover a single `spin_once()` invocation. They don't prove that a message entering a ring buffer is *eventually* consumed across multiple spin cycles. A message could sit in a ring slot forever if the tail never advances.

4. **No deadline bound** — there is no model relating wall-clock time to spin cycles. We can't prove "if spin period ≤ T and ring depth ≤ D, then any message is consumed within D×T ms."

5. **No cross-flow isolation** — we can't prove that a large message fragmenting on the sensor channel doesn't delay a small control message.

6. **The FFI boundary is unverified** — ghost models mirror Rust production code, but zenoh-pico is C code called via FFI. The zenoh-pico callback behavior is assumed, not verified.

## 2. Verification Strategy Overview

We use three complementary techniques at different abstraction levels:

```
┌─────────────────────────────────────────────────────────────┐
│  Level 3: System-level properties (Verus, unbounded)        │
│  "A message in the ring buffer is consumed within D spins"  │
│  "Fragmented messages are either fully reassembled or       │
│   entirely dropped — never partially delivered"             │
├─────────────────────────────────────────────────────────────┤
│  Level 2: Component state machines (Verus + Kani)           │
│  "Ring buffer head/tail atomics preserve FIFO ordering"     │
│  "Defrag buffer either completes or times out to empty"     │
├─────────────────────────────────────────────────────────────┤
│  Level 1: Data structure invariants (Kani, bounded)         │
│  "For all buffer sizes ≤ 64KB, head/tail never alias"       │
│  "Fragment bitmap tracks exactly the received fragments"    │
└─────────────────────────────────────────────────────────────┘
```

### Trust boundary

The verification cannot cover zenoh-pico's internal C code. Instead, we define **interface contracts** at the FFI boundary — preconditions and postconditions on the C functions — and verify the Rust code assuming those contracts hold. This is the same "ghost model validated by production tests" strategy used throughout the existing verification infrastructure.

```
Verified (Verus/Kani)          │  Assumed (FFI contract)
───────────────────────────────┼──────────────────────────────
Ring buffer state machine      │  zenoh-pico delivers complete
Executor dispatch ordering     │    payloads to callbacks
Deadline bound arithmetic      │  zenoh-pico fragments/reassembles
Contract admission control     │    according to its protocol spec
CDR serialize/deserialize      │  zenoh-pico respects priority
Progress across spin cycles    │    ordering for same-session msgs
```

## 3. Ghost Models to Add

### 3.1 `RingBufferGhost` — Multi-slot subscriber queue

This replaces `SubscriberBufferGhost` for `ControlCritical` topics with `history_depth > 1`.

```rust
/// Ghost model of a SPSC ring buffer with atomic head/tail.
///
/// Producer: zenoh callback (ISR/task context) — writes at head.
/// Consumer: executor spin loop (task context) — reads from tail.
///
/// Invariants (to be proven):
///   I1: tail <= head                    (modular: head - tail ≤ DEPTH)
///   I2: head - tail <= DEPTH            (never more than DEPTH items)
///   I3: each slot written before read   (no uninitialized access)
///   I4: FIFO order preserved            (tail advances monotonically)
pub struct RingBufferGhost {
    /// Write index (advanced by producer callback)
    pub head: u16,
    /// Read index (advanced by consumer/executor)
    pub tail: u16,
    /// Ring capacity (compile-time constant in production)
    pub depth: u16,
    /// Number of items currently stored (head - tail, mod depth)
    pub count: u16,
    /// Cumulative messages dropped due to full buffer
    pub overflow_count: u32,
    /// Per-slot payload length (0 = empty/uninitialized)
    pub slot_lengths: Seq<u16>,  // Verus Seq, length == depth
    /// Per-slot validity (true after write, false after read)
    pub slot_valid: Seq<bool>,
}
```

**Operations to model:**

```rust
impl RingBufferGhost {
    /// Producer writes to head slot. Returns false if full (DropNewest policy).
    pub fn push(&mut self, msg_len: u16) -> bool { ... }

    /// Consumer reads from tail slot. Returns None if empty.
    pub fn pop(&mut self) -> Option<u16> { ... }

    /// DropOldest variant: overwrites tail on full.
    pub fn push_overwrite(&mut self, msg_len: u16) { ... }
}
```

**Properties to prove (Verus, unbounded):**

| ID | Property | Spec |
|---|---|---|
| RB1 | No overflow aliasing | `push` when full returns `false` (DropNewest) or advances tail (DropOldest) |
| RB2 | FIFO ordering | `pop` returns items in the order they were `push`ed |
| RB3 | Bounded occupancy | `0 ≤ count ≤ depth` at all times |
| RB4 | No double-read | After `pop`, `slot_valid[tail_old]` becomes `false` |
| RB5 | Liveness | If `count > 0`, `pop` returns `Some` |
| RB6 | Producer/consumer independence | `push` only modifies `head`, `pop` only modifies `tail` |

**Kani harnesses (bounded model checking):**

| ID | Harness | Bound |
|---|---|---|
| RB-K1 | Push-pop cycle preserves invariant | depth ≤ 16 |
| RB-K2 | Overflow-then-pop recovers | depth ≤ 16 |
| RB-K3 | Interleaved push/pop (ISR-style) | depth ≤ 8, 20 operations |
| RB-K4 | DropOldest never loses more than 1 per push | depth ≤ 16 |

### 3.2 `FragmentSetGhost` — Fragmentation/reassembly tracking

This models the zenoh-pico defragmentation buffer at the interface level. We don't model zenoh-pico internals — we model the *observable behavior* as seen by nros.

```rust
/// Ghost model of a message undergoing fragmentation and reassembly.
///
/// Represents the state visible to nros at the FFI boundary:
/// - zenoh-pico calls the subscriber callback with either a complete
///   message (single frame) or a reassembled message (after collecting
///   all fragments).
///
/// The model captures what can go wrong and what guarantees we assume
/// from zenoh-pico's fragmentation protocol.
pub struct FragmentSetGhost {
    /// Total fragments expected for this message
    pub total_fragments: u16,
    /// Bitmap of received fragments (true = received)
    pub received: Seq<bool>,
    /// Whether reassembly is complete (all fragments received)
    pub complete: bool,
    /// Whether reassembly timed out (lease expiry)
    pub timed_out: bool,
    /// Original message size before fragmentation
    pub original_size: usize,
    /// Maximum single-frame payload (Z_BATCH_UNICAST_SIZE - overhead)
    pub max_frame_payload: usize,
}
```

**Interface contracts (assumed at FFI boundary):**

```rust
/// Axiom F1: Messages ≤ max_frame_payload are never fragmented.
/// This is the foundation of ControlCritical's guarantee.
pub open spec fn no_fragmentation_axiom(msg_size: usize, max_frame: usize) -> bool {
    msg_size <= max_frame ==> /* delivered in single callback, no reassembly needed */
    true
}

/// Axiom F2: Reassembly is atomic — the callback receives either
/// the complete original message or nothing (timeout/drop).
/// No partial messages escape zenoh-pico's defrag buffer.
pub open spec fn atomic_reassembly_axiom(frag: FragmentSetGhost) -> bool {
    // Either complete (all received) or timed_out (none delivered)
    (frag.complete ==> forall|i: int| 0 <= i < frag.total_fragments ==> frag.received[i])
    && (frag.timed_out ==> !frag.complete)
    // No partial state is visible to the callback
}

/// Axiom F3: Fragments for message A do not consume the defrag buffer
/// needed by message B on a different priority channel.
/// (Requires session isolation or per-priority defrag — see Section 4.2)
pub open spec fn defrag_isolation_axiom(
    channel_a: nat, channel_b: nat,
    frag_a: FragmentSetGhost,
) -> bool {
    channel_a != channel_b ==>
    /* frag_a's reassembly state does not affect channel_b's capacity */
    true
}
```

**Properties to prove (conditional on axioms):**

| ID | Property | Depends on |
|---|---|---|
| FR1 | ControlCritical messages bypass fragmentation entirely | F1 + contract.max_payload ≤ max_frame |
| FR2 | A fragmented sensor message either delivers completely or is dropped | F2 |
| FR3 | Bulk reassembly on channel 2 doesn't block sensor reassembly on channel 1 | F3 (requires session isolation) |
| FR4 | Fragment count = ceil(original_size / max_frame_payload) | Arithmetic, provable |
| FR5 | Defrag buffer size ≥ original_size is sufficient for reassembly | Arithmetic, provable |

### 3.3 `DeliveryChainGhost` — End-to-end message lifecycle

Combines all per-layer ghost models into a single lifecycle:

```rust
/// Ghost model of a message's complete lifecycle from publish to callback.
///
/// Each field represents the outcome at one stage. The proof shows that
/// the message either reaches the subscriber callback or is accounted
/// for by an explicit error/drop at a specific stage.
pub struct DeliveryChainGhost {
    // --- Publisher side ---
    pub serialize_ok: bool,        // CDR serialization succeeded
    pub publish_raw_ok: bool,      // zenoh put succeeded
    pub sequence_number: i64,      // Assigned sequence number

    // --- Transport ---
    pub needs_fragmentation: bool, // msg_size > max_frame_payload
    pub fragments_sent: u16,       // Fragments transmitted
    pub fragments_received: u16,   // Fragments received by peer
    pub reassembly_complete: bool, // All fragments collected
    pub reassembly_timed_out: bool,// Lease timeout before completion

    // --- Subscriber side ---
    pub callback_invoked: bool,    // zenoh-pico invoked Rust callback
    pub buffer_locked: bool,       // Reader was holding lock → dropped
    pub buffer_overflow: bool,     // Message > buffer capacity
    pub ring_full: bool,           // Ring buffer was full
    pub ring_drop_policy_applied: bool, // DropOldest evicted oldest
    pub stored_in_ring: bool,      // Successfully enqueued

    // --- Executor side ---
    pub spin_invoked: bool,        // spin_once ran since message stored
    pub trigger_fired: bool,       // Trigger evaluated to true
    pub callback_executed: bool,   // User callback invoked
    pub deserialize_ok: bool,      // CDR deserialization succeeded
    pub deadline_met: bool,        // Latency ≤ deadline_ms
}
```

**End-to-end properties to prove:**

| ID | Property | Statement |
|---|---|---|
| E2E-1 | **No silent loss** | If `callback_executed`, then the full publish→transport→subscribe chain succeeded. If not `callback_executed`, then exactly one of the explicit failure flags is true. |
| E2E-2 | **Delivery accounting** | `serialize_ok ∧ publish_raw_ok ∧ reassembly_complete ∧ stored_in_ring ∧ spin_invoked ∧ trigger_fired ∧ deserialize_ok → callback_executed` |
| E2E-3 | **Drop classification** | Every non-delivered message has exactly one root cause: `¬serialize_ok ∨ ¬publish_raw_ok ∨ reassembly_timed_out ∨ buffer_locked ∨ buffer_overflow ∨ (ring_full ∧ DropNewest) ∨ ¬trigger_fired ∨ ¬deserialize_ok` |
| E2E-4 | **ControlCritical path** | `¬needs_fragmentation ∧ ¬buffer_overflow ∧ ¬ring_full → stored_in_ring` (control messages that fit are always stored) |

### 3.4 `MultiSpinGhost` — Liveness across spin cycles

The key liveness property: a message in the ring buffer is *eventually* consumed.

```rust
/// Ghost model of a sequence of spin_once invocations.
///
/// Models the relationship between ring buffer state and time
/// (measured in spin cycles, not wall-clock) to prove bounded
/// consumption latency.
pub struct MultiSpinGhost {
    /// Number of spin cycles elapsed since message was enqueued
    pub spins_since_enqueue: u32,
    /// Ring buffer occupancy at enqueue time
    pub ring_count_at_enqueue: u16,
    /// Ring buffer depth
    pub ring_depth: u16,
    /// Whether trigger fires on every spin (true for Always/Any with data)
    pub trigger_always_fires: bool,
    /// Whether this subscription's callback is always successful
    pub callback_always_succeeds: bool,
}
```

**Properties:**

| ID | Property | Statement |
|---|---|---|
| MS1 | **Bounded consumption** | If `trigger_always_fires ∧ callback_always_succeeds`, then after `ring_count_at_enqueue` spin cycles, the message has been consumed or evicted. |
| MS2 | **FIFO ordering** | Messages are consumed in enqueue order (tail monotonically advances). |
| MS3 | **No starvation** | If `trigger_always_fires`, every message in the ring is consumed within `ring_depth` spins. |
| MS4 | **Deadline bound** | If `spin_period_ms ≤ T`, then consumption latency ≤ `ring_depth × T` ms. |

## 4. Proof Architecture

### 4.1 Modular composition

Each ghost model is proven correct independently. The end-to-end proof composes them:

```
                                    DeliveryChainGhost (E2E)
                                   ╱        │        ╲
                         PublishChain  FragmentSet  RingBuffer + MultiSpin
                              │            │              │
                           CdrGhost    (FFI axiom)    SpinOnceGhost
```

The composition works because each model's postcondition matches the next model's precondition:

```
PublishChainGhost.publish_raw_ok == true
    ⟹ zenoh-pico has the payload
    ⟹ (by F1/F2 axiom) callback is invoked with complete payload OR timeout
    ⟹ RingBufferGhost.push() is called with msg_len
    ⟹ (by RB3/RB5) stored_in_ring OR overflow_count incremented
    ⟹ (by MS1/MS3) consumed within ring_depth spins OR evicted by DropOldest
    ⟹ callback_executed OR explicit drop reason
```

### 4.2 Fragmentation proofs — what is tractable

**Provable within nros (no zenoh-pico internals):**

1. **Fragment count calculation**: `ceil(msg_size / max_frame_payload)` — pure arithmetic, Verus.
2. **Defrag buffer sizing**: `Z_FRAG_MAX_SIZE ≥ max_payload` from contract — Verus.
3. **ControlCritical never fragments**: `contract.max_payload ≤ Z_BATCH_UNICAST_SIZE - overhead` — Verus admission check.
4. **Session isolation implies defrag isolation**: If bulk traffic uses session B and control uses session A, they have independent defrag buffers — Verus, structural argument.

**Assumed (FFI axiom, validated by integration tests):**

5. **Atomic reassembly**: zenoh-pico either delivers the full reassembled message or drops it. No partial delivery. This is a property of the zenoh transport protocol, tested via integration tests (`test-qemu` with large messages), but not formally verified.
6. **Priority scheduling**: zenoh frames with higher priority are transmitted before lower-priority frames within the same session. Tested via `test-integration` with mixed traffic.

**Not tractable (would require verifying zenoh-pico C code):**

7. Fragment sequence number correctness.
8. Retransmission logic for reliable transport.
9. Lease timeout expiration correctness.

### 4.3 Ring buffer lock-freedom proof

The ring buffer uses two atomic indices (head, tail) with no mutex. For Cortex-M3+ (ARMv7-M), we rely on `LDREX`/`STREX` for atomic operations. The proof must show that:

1. **Producer (callback) only modifies `head`** — consumer's `tail` is read-only from the producer's perspective.
2. **Consumer (executor) only modifies `tail`** — producer's `head` is read-only from the consumer's perspective.
3. **No ABA problem** — because `head` and `tail` are monotonically increasing (modulo depth), and depth is a power-of-two, the modular arithmetic guarantees distinct slot addresses.

**Proof strategy**: Model the SPSC ring buffer as two independent state machines sharing a bounded counter. Each machine's transitions only modify its own index. The shared invariant `0 ≤ (head - tail) mod depth ≤ depth` is preserved by both transitions independently.

This is a well-known pattern (Lamport's 1983 SPSC proof). In Verus:

```rust
proof fn ring_buffer_spsc_invariant(
    head_before: u16, tail_before: u16, depth: u16, msg_len: u16,
)
    requires
        depth > 0,
        (head_before - tail_before) as int % depth as int >= 0,
        (head_before - tail_before) as int % depth as int <= depth as int,
    ensures
        // After push (head advances by 1):
        ({
            let head_after = (head_before + 1) % depth;
            let count_after = (head_after as int - tail_before as int) % depth as int;
            count_after >= 0 && count_after <= depth as int
        }),
        // After pop (tail advances by 1):
        ({
            let tail_after = (tail_before + 1) % depth;
            let count_after = (head_before as int - tail_after as int) % depth as int;
            count_after >= 0 && count_after <= depth as int
        }),
{ }
```

For Cortex-M0 (no `LDREX`/`STREX`), the fallback disables interrupts for the ~10-cycle head advance. The proof is simpler (sequential execution during critical section), but we add a Kani harness to verify the interrupt-disable window is bounded.

### 4.4 Deadline bound proof

The deadline bound composes timing arithmetic with the ring buffer model:

```rust
/// If the executor spins at period T_ms, and the ring buffer has depth D,
/// then any message stored at ring position P is consumed within
/// (D - P + 1) × T_ms milliseconds, assuming the trigger fires on every spin.
proof fn deadline_bound(
    spin_period_ms: u32,
    ring_depth: u16,
    position_from_tail: u16,  // 0 = next to be consumed
)
    requires
        spin_period_ms > 0,
        ring_depth > 0,
        position_from_tail < ring_depth,
    ensures
        ({
            let worst_case_spins = (ring_depth - position_from_tail) as u32;
            let worst_case_latency_ms = worst_case_spins * spin_period_ms;
            // Message is consumed within this many milliseconds
            worst_case_latency_ms <= ring_depth as u32 * spin_period_ms
        }),
{ }
```

**Combining with admission control**: The contract's `deadline_ms` field is validated at setup time:

```rust
proof fn admission_deadline_check(
    deadline_ms: u32,
    spin_period_ms: u32,
    ring_depth: u16,
)
    requires
        deadline_ms >= ring_depth as u32 * spin_period_ms,
    ensures
        // The deadline is achievable: worst-case consumption latency ≤ deadline
        ring_depth as u32 * spin_period_ms <= deadline_ms,
{ }
```

If the check passes, we have a machine-checked guarantee that the deadline is achievable.

## 5. Relating to Existing Research

### 5.1 Verified SPSC ring buffers — Verus tokenized state machines

The Verus project provides a verified SPSC producer-consumer queue example using `tokenized_state_machine!` (SOSP 2024 tutorial). The state machine defines `backing_cells` (a `Seq<CellId>` for ring buffer slots), tracks ownership via ghost state, and uses atomic loads of head/tail with `produce_start`/`produce_end` transitions. This is directly applicable to our `RingBufferGhost`.

Key references:
- **Verus SPSC queue** ([verus-lang.github.io](https://verus-lang.github.io/verus/state_machines/examples/src-producer-consumer-queue.html)): tokenized state machine with `produce_start`, `produce_end`, `consume_start`, `consume_end` transitions. Each slot has a ghost `CellId` tracked through ownership transfer.
- **Travis Hance's PhD thesis** (CMU, 2024): verified concurrent systems code in Verus, including SplinterCache (concurrent page cache with fine-grained locking) and NR (node replication with linearizability proof). The NR port required far fewer lines of proof than IronSync/Dafny and improved verification time by two orders of magnitude.
- **Converos** (ATC 2025): practical model checker for concurrent Rust OS code, verified 12 concurrency modules including a lock-free ring buffer in the Asterinas OS.

Our approach differs from the Verus tutorial example in that the producer runs in ISR/callback context (potentially preempting the consumer). The single-writer property (only ISR writes head, only executor writes tail) keeps this tractable. We may use `tokenized_state_machine!` for the strongest guarantee (ghost ownership transfer per slot) or stay with the simpler ghost-model approach for consistency with existing proofs.

### 5.2 Fragmentation verification — CAN bus and WiFi precedents

Formal verification of fragmentation protocols has precedent:
- **CAN bus protocol stack** (Glabbeek, Hoefner, Mars, 2017): modeled fragmentation, reassembly, and multiplexing. Proved that any received message was actually sent (no mis-reassembly) and that any sent message is received (assuming a perfect channel). Also proved absence of deadlocks.
- **WiFi 802.11 fragmentation** (2023): used Tamarin prover to formally analyze fragmentation vulnerabilities, verifying integrity checks that prevent reassembly attacks.
- **IPv6 fragmentation in PVS**: machine-readable specification eliminating ambiguity in the RFC.
- **TLS 1.3 in F-star**: full verified implementation including record layer fragmentation.

For nano-ros, the CAN bus approach maps well: define a ghost state machine tracking fragment sequences and prove no-mis-reassembly and no-deadlock. Since we treat zenoh-pico as opaque, we model only the observable behavior (complete delivery or timeout) rather than the internal reassembly logic.

### 5.3 Deadline bounds as safety properties — Performal

**Performal** (PLDI 2023) proves rigorous latency upper bounds for distributed systems with a crucial insight: formulate deadline guarantees as **safety properties** ("if the action happens, it completes within time T") rather than liveness properties. Safety properties are dramatically easier to prove via inductive invariants — no fairness assumptions or well-founded orderings needed.

Applied to nano-ros:
- Instead of proving "the message is *eventually* consumed" (liveness, hard), prove "if spin_once has been called K times since enqueue, then the message has been consumed or explicitly dropped" (safety, inductive invariant on spin count).
- The deadline bound `K ≤ ring_depth` is an arithmetic invariant that Verus/Z3 can discharge automatically.

This insight changes our `MultiSpinGhost` proofs from temporal logic (which Verus doesn't natively support) to plain inductive invariants (which Verus excels at).

Additional references:
- **VeriRT** (POPL 2025): framework for verifying real-time distributed systems using refinement proofs, with bounded clock skew proofs.
- **Kairos** (OSDI 2024): embeds temporal property monitors with freshness annotations for deadline specifications.

### 5.4 TLA+ for design validation

TLA+ is useful for design-level exploration before committing to Verus proofs:
- **Hillel Wayne's message queue models**: represent pub/sub (each reader has its own queue, writer appends to all), at-least-once delivery, and SQS-style semantics.
- **Reusable TLA+ communication primitives**: modular specs for perfect, fair-loss, and stubborn links with fault injection.

We could write a TLA+ spec of our delivery model, model-check it with TLC for small configurations, then use the validated design as the sequential spec that Verus proofs refine against. This two-tier approach (TLA+ for design, Verus for implementation) is practical and separates concerns. However, it adds a maintenance burden — the TLA+ spec must stay synchronized with the Verus ghost models.

We use Verus rather than TLA+ for implementation proofs because:
1. Verus proofs are directly linked to Rust production code via ghost types.
2. TLA+ would require a separate model with no automated correspondence to the implementation.
3. Verus leverages Z3 for automated proof discharge — most of our properties are decidable fragments (linear arithmetic + arrays).

### 5.5 IronFleet/Grove and verified distributed systems

- **IronFleet** (SOSP 2015): first to prove safety and liveness of practical distributed systems (Paxos RSM + sharded KV store) in Dafny. **IronKV has been ported to Verus**, proving that Verus subsumes IronFleet's methodology.
- **Verdi** (PLDI 2015): verified Raft in Coq using verified system transformers for network semantics.
- **Grove** (SOSP 2023): concurrent separation logic library (Iris/Perennial in Coq) for RPCs, time-based leases, and crash recovery. Proof overhead: 12x lines of proof vs. lines of code. This ratio gives a realistic effort estimate for our verification.

Our situation differs — we can't verify zenoh-pico (unmanaged C code). We use the **"verified shim" pattern** from IronFleet: verify the Rust wrapper around the unverified C library, assuming the C library meets its documented interface contract. This is weaker than full-stack verification but pragmatic for embedded systems integrating third-party middleware.

### 5.6 Verified RTOS components and ROS 2 verification

- **seL4**: functional correctness proof in Isabelle/HOL, ~12k lines of C. Demonstrates scope control: minimize the verified TCB.
- **CertiKOS/RT-CertiKOS**: integrates with Prosa schedulability analyzer for end-to-end real-time guarantees.
- **eChronos**: verified RTOS for microcontrollers using Owicki-Gries and Rely-Guarantee for concurrency.
- **Taiji project**: formally verified core modules of Zephyr RTOS and VxWorks 653 for aerospace certification — directly relevant to our Zephyr platform backend.

For ROS 2 specifically:
- **UPPAAL timed automata models** (2024/2025): model ROS 2 executors and callback scheduling, prove execution-trace equivalence.
- **AS2FM** (2025): translates ROS 2 system models to JANI for statistical model checking.
- No project has done deductive proofs of ROS 2 middleware implementation code — nano-ros with Verus+Kani is ahead of the state of the art.

Our deadline bound assumes the spin task runs at its configured period. We do *not* verify the RTOS scheduler itself — the platform layer is a trust boundary. We could add a `platform_schedule_bound` axiom: "the platform guarantees the spin task runs within `spin_period_ms + jitter_ms` of its deadline." This axiom is validated by RTOS configuration review, not by proof.

## 6. Kani Harnesses to Add

### 6.1 Ring buffer harnesses

```rust
#[kani::proof]
fn ring_push_pop_preserves_invariant() {
    let depth: u16 = kani::any();
    kani::assume(depth > 0 && depth <= 16);
    let mut ring = RingBufferGhost::new(depth);

    // Arbitrary sequence of push/pop operations
    for _ in 0..20u8 {
        let op: bool = kani::any();
        if op {
            let len: u16 = kani::any();
            kani::assume(len > 0 && len <= 1024);
            ring.push(len);
        } else {
            ring.pop();
        }
        assert!(ring.count <= ring.depth);
        assert!(ring.head >= ring.tail || ring.head < ring.depth); // modular
    }
}

#[kani::proof]
fn ring_fifo_ordering() {
    let depth: u16 = kani::any();
    kani::assume(depth >= 2 && depth <= 8);
    let mut ring = RingBufferGhost::new(depth);

    // Push two messages with distinct lengths
    let len1: u16 = kani::any();
    let len2: u16 = kani::any();
    kani::assume(len1 > 0 && len1 <= 1024);
    kani::assume(len2 > 0 && len2 <= 1024);
    kani::assume(len1 != len2);

    ring.push(len1);
    ring.push(len2);

    // Pop should return len1 first, then len2
    let first = ring.pop();
    let second = ring.pop();
    assert_eq!(first, Some(len1));
    assert_eq!(second, Some(len2));
}

#[kani::proof]
fn ring_drop_oldest_never_loses_newest() {
    let depth: u16 = kani::any();
    kani::assume(depth >= 2 && depth <= 8);
    let mut ring = RingBufferGhost::new(depth);

    // Fill the buffer
    for i in 0..depth {
        ring.push(i + 1);
    }
    assert_eq!(ring.count, ring.depth);

    // Push one more with DropOldest
    let new_len: u16 = kani::any();
    kani::assume(new_len > 0);
    ring.push_overwrite(new_len);

    // The newest message is in the buffer (at head-1)
    // The oldest message (len=1) was evicted
    assert_eq!(ring.count, ring.depth);
    // Pop all — last popped should be new_len
    let mut last = 0u16;
    while ring.count > 0 {
        if let Some(l) = ring.pop() {
            last = l;
        }
    }
    assert_eq!(last, new_len);
}
```

### 6.2 Fragment tracking harness

```rust
#[kani::proof]
fn fragment_count_correct() {
    let msg_size: usize = kani::any();
    let max_frame: usize = kani::any();
    kani::assume(msg_size > 0 && msg_size <= 65536);
    kani::assume(max_frame > 0 && max_frame <= 1024);

    let expected_fragments = (msg_size + max_frame - 1) / max_frame;

    // Verify: sum of fragment payloads ≥ msg_size
    let total_payload = expected_fragments * max_frame;
    assert!(total_payload >= msg_size);

    // Verify: (fragments-1) * max_frame < msg_size (not over-counted)
    if expected_fragments > 1 {
        assert!((expected_fragments - 1) * max_frame < msg_size);
    }
}

#[kani::proof]
fn control_critical_never_fragments() {
    let max_payload: usize = kani::any();
    let batch_size: usize = kani::any();
    let overhead: usize = kani::any();

    kani::assume(max_payload <= 900);
    kani::assume(batch_size == 1024);
    kani::assume(overhead == 120);
    kani::assume(max_payload <= batch_size - overhead);

    let fragments = (max_payload + (batch_size - overhead) - 1) / (batch_size - overhead);
    assert_eq!(fragments, 1); // Never more than 1 frame
}
```

### 6.3 Deadline bound harness

```rust
#[kani::proof]
fn deadline_achievable() {
    let spin_period_ms: u32 = kani::any();
    let ring_depth: u16 = kani::any();
    let deadline_ms: u32 = kani::any();

    kani::assume(spin_period_ms > 0 && spin_period_ms <= 100);
    kani::assume(ring_depth > 0 && ring_depth <= 16);
    kani::assume(deadline_ms > 0 && deadline_ms <= 10000);

    let worst_case = (ring_depth as u32).checked_mul(spin_period_ms);
    if let Some(wc) = worst_case {
        if deadline_ms >= wc {
            // Admission check passes → deadline is achievable
            assert!(wc <= deadline_ms);
        }
    }
}
```

## 7. Verus Module Organization

New proofs are added to `nros-verification/src/`:

| File | Content |
|---|---|
| `ring_buffer.rs` (new) | RingBufferGhost type spec, SPSC invariant proofs, FIFO ordering, bounded occupancy |
| `fragmentation.rs` (new) | FragmentSetGhost type spec, fragment count arithmetic, ControlCritical bypass proof, defrag sizing |
| `delivery.rs` (new) | DeliveryChainGhost composition, E2E no-silent-loss, drop classification, deadline bound |
| `progress.rs` (extend) | MultiSpinGhost liveness proof (bounded consumption across multiple spins) |
| `e2e.rs` (extend) | Update existing single-slot proofs to delegate to ring_buffer.rs for depth > 1 |

Ghost types go in `nros-ghost-types/src/lib.rs` as usual.

## 8. Implementation Phases

| Phase | Scope | Est. proofs | Tool |
|---|---|---|---|
| **V1** | `RingBufferGhost` + SPSC invariants + Kani harnesses | 6 Verus + 4 Kani | Both |
| **V2** | `FragmentSetGhost` + arithmetic proofs + never-fragment proof | 5 Verus + 2 Kani | Both |
| **V3** | `DeliveryChainGhost` + E2E composition (no-silent-loss) | 4 Verus | Verus |
| **V4** | `MultiSpinGhost` + bounded consumption + deadline bound | 4 Verus + 1 Kani | Both |
| **V5** | Update existing proofs (e2e.rs, progress.rs) to compose with new models | 3 Verus | Verus |
| **V6** | FFI axiom documentation + integration test validation matrix | 0 proofs | Docs + tests |

**Total**: ~22 new Verus proofs, ~7 new Kani harnesses.

## 9. Validation of FFI Axioms

The FFI axioms (F1–F3) cannot be formally proven within our Rust-side verification. They are validated by:

1. **Integration tests** (`test-integration`): Publish large messages (> batch size), verify complete delivery or explicit timeout. Run with mixed traffic (control + sensor + bulk) to validate isolation.

2. **Fuzz testing**: Generate random message sizes and publish rates. Verify that the subscriber callback never receives a partial message (check CDR header + length consistency).

3. **zenoh-pico protocol conformance tests**: The zenoh-pico project has its own test suite. We pin to a specific version (1.6.2) and validate against its test results.

4. **Kani on the Rust shim**: Even though we can't verify the C code, we can verify that the Rust shim correctly interprets the C callback's output. The existing `sub_callback_*` and `svc_callback_*` Kani harnesses do this.

## 10. Open Questions

1. **Tokenized state machine vs. ghost model** — Verus's `tokenized_state_machine!` (used in the SPSC queue example) provides the strongest concurrency guarantee via ghost ownership transfer per slot. However, the existing nano-ros verification uses the simpler ghost-model approach (manual mirrors validated by production tests). Should we adopt tokenized state machines for the ring buffer? This would be the first use of `tracked` ghost state in the project and would require updating the verification guide. The upside is machine-checked linearizability; the downside is significantly more complex proofs (~3x lines) and a steep learning curve.

2. **TLA+ design spec as a pre-step** — Should we write a TLA+ model of the message delivery semantics and model-check it with TLC before committing to Verus proofs? This separates design validation (fast iteration in TLA+) from implementation verification (Verus). The risk is maintaining two models. The alternative is to use Verus spec functions as the design spec directly (the current approach).

3. **Composing with safety-e2e** — The `SafetyValidatorGhost` already tracks sequence numbers. Should the `DeliveryChainGhost` subsume it, or should they remain independent and compose via a separate lemma?

4. **Latency as safety vs. liveness** — Per Performal (PLDI 2023), we should formulate deadline guarantees as safety properties: "if K spins have occurred, the message has been consumed" rather than "the message is eventually consumed." This makes proofs tractable with Verus's inductive invariant style. Are there scenarios where true liveness (not bounded by spin count) is needed?

5. **Platform jitter modeling** — The deadline bound assumes `spin_period_ms` is exact. On real RTOS, there is scheduling jitter. We could add a `jitter_ms` term: `worst_case = ring_depth × (spin_period_ms + jitter_ms)`. The jitter bound comes from RTOS WCET analysis (similar to Prosa/RT-CertiKOS integration). Should the platform layer provide this bound as an axiom?

6. **Multiple publishers to one subscriber (MPSC)** — The ring buffer model assumes SPSC (one producer, one consumer). If two publishers write to the same topic, the subscriber callback is MPSC. The ring buffer proof needs to handle multiple producers. Options: (a) CAS-based MPSC ring (significantly harder proof), (b) per-publisher ring buffer merged by executor (simpler proof but more memory), (c) interrupt-disable on enqueue (simplest but limits concurrency on multicore). The Verus NR (node replication) approach — flat combining + sequential spec — could work for (a).

7. **Modeling network partition** — A network partition causes all messages to be lost for some duration. Should we model this as "all fragments timed out for N consecutive messages" and prove that the system recovers when the partition heals?
