# Tokenized State Machine Study for nano-ros

This document analyzes how Verus's `tokenized_state_machine!` macro works and
evaluates whether/how it could be applied to the nano-ros subscriber buffer.

## 1. How `tokenized_state_machine!` Works

### 1.1 Core idea

A `tokenized_state_machine!` defines a global protocol as a state machine, then
**shatters** (shards) its state into individual **tokens**. Each token is a `tracked`
value that can be held by a different thread. A transition is an **exchange** of
tokens — you hand in some tokens and get back others, and Verus proves the
exchange preserves the global invariant.

The key property: **if you hold a token, its value accurately reflects part of
the real global state**. You cannot fabricate tokens, so the invariant is
compositional — each thread's reasoning is local, but the global invariant holds.

### 1.2 Sharding strategies

Each field in the state machine gets a **sharding strategy** that determines how
the field's value is split across tokens:

| Strategy | Token type | Semantics |
|---|---|---|
| `constant` | `Instance` (shared) | Fixed at init, never changes. Available to anyone with an `Instance`. |
| `variable` | `State::field` (unique) | Exactly one token exists. Whoever holds it owns the field. Like a linear resource. |
| `option` | `State::field` (fractional) | `None` = no token, `Some(v)` = one token. Can be split/merged. |
| `map` | `State::field` (per-key) | Map from keys to tokens. Each key is independently owned. |
| `storage_option` | *(no new token)* | Stores a token from **another** system (e.g., `PointsTo`). Deposited/withdrawn by transitions. |
| `storage_map` | *(no new token)* | Like `storage_option` but keyed. Stores a `Map<K, Tok>` of external tokens. |

The critical distinction: `variable`/`option`/`map` generate new token types;
`storage_option`/`storage_map` store tokens from *other* systems (like cell
permissions). This is how the state machine links to real memory.

### 1.3 The Verus SPSC queue in detail

The verified FIFO queue has these fields:

```rust
tokenized_state_machine!{FifoQueue<T> {
    fields {
        #[sharding(constant)]    pub backing_cells: Seq<CellId>,       // cell IDs (fixed)
        #[sharding(storage_map)] pub storage: Map<nat, cell::PointsTo<T>>, // cell permissions
        #[sharding(variable)]    pub head: nat,                        // shared atomic
        #[sharding(variable)]    pub tail: nat,                        // shared atomic
        #[sharding(variable)]    pub producer: ProducerState,          // producer-local
        #[sharding(variable)]    pub consumer: ConsumerState,          // consumer-local
    }
```

**How tokens map to the implementation:**

| Token | Held by | Runtime representation |
|---|---|---|
| `Instance` | Everyone (via `Arc<Queue>`) | Shared identity; immutable `backing_cells` |
| `FifoQueue::head` | Inside `AtomicU64` ghost state | Updated via `atomic_with_ghost!` |
| `FifoQueue::tail` | Inside `AtomicU64` ghost state | Updated via `atomic_with_ghost!` |
| `FifoQueue::producer` | `Producer` struct (`Tracked` field) | Producer-local, not shared |
| `FifoQueue::consumer` | `Consumer` struct (`Tracked` field) | Consumer-local, not shared |
| `cell::PointsTo<T>` | In `storage_map` or checked out | Permission to read/write a `PCell` |

**Transition flow for `enqueue(t)`:**

```
                           State machine tokens
                           ─────────────────────
1. Producer reads head     atomic_with_ghost!(&queue.head => load(); ghost head_token => {
   from atomic                // Inside atomic invariant, we can read head_token
                              // and call produce_start transition
   produce_start:             instance.produce_start(&head_token, &mut producer_token);
     require producer == Idle(tail)
     require inc_wrap(tail) != head        ← checks not-full
     update producer = Producing(tail)
     withdraw storage -= [tail => perm]    ← checks out the PointsTo for slot[tail]
                              // Returns perm: cell::PointsTo<T> (tracked, uninit)
                           });

2. Producer writes data    queue.buffer[tail].put(Tracked(&mut perm), t);
   into the cell              // put() consumes the uninit PointsTo, produces init PointsTo
                              // This is enforced by PCell's API — you can't write
                              // without the permission token

3. Producer updates tail   atomic_with_ghost!(&queue.tail => store(next_tail); ghost tail_token => {
   atomically                 instance.produce_end(perm,     ← deposits the now-init PointsTo
                                &mut tail_token,             ← updates tail value
                                &mut producer_token);        ← moves to Idle(next_tail)
                           });
```

**Why this is sound:**

- Step 1: The `produce_start` transition **withdraws** the `PointsTo` from
  `storage_map`. After this, no one else can access the cell — the permission
  is exclusively held by the producer.
- Step 2: `PCell::put` requires a `&mut PointsTo<T>` that is `uninit`. This is
  a type-level proof obligation — you can't call `put` without the right
  permission. After `put`, the permission becomes `init`.
- Step 3: The `produce_end` transition **deposits** the now-init `PointsTo` back
  into `storage_map`. The consumer can later withdraw it.

The consumer follows the mirror: `consume_start` withdraws an init permission,
reads the data via `PCell::take`, then `consume_end` deposits the now-uninit
permission back.

### 1.4 Key Verus APIs

- **`PCell<T>`**: A verified equivalent of `UnsafeCell<MaybeUninit<T>>`. Each
  `PCell` has a unique `CellId`. Access requires a `cell::PointsTo<T>` token
  matching that `CellId`. The token tracks whether the cell is `init` or `uninit`.
- **`AtomicU64<_, Ghost, _>`**: An atomic integer that carries ghost state. The
  `atomic_with_ghost!` macro opens the atomic invariant, giving access to the
  ghost token while holding the atomic.
- **`Tracked<T>`**: A compile-time-only (`ghost`) value. `Tracked` fields exist
  in the type signature but are erased at runtime — zero overhead.
- **`Instance`**: A token identifying a particular state machine instance.
  Ensures tokens from different instances can't be mixed.

## 2. Current nano-ros Approach vs. Tokenized State Machine

### 2.1 Current approach: Ghost model + 3-layer validation

```
Production code (SubscriberBuffer)
    │  AtomicBool has_data, overflow, locked
    │  AtomicUsize len
    │  [u8; 1024] data
    │
    ├── Layer 1: ghost_from_buffer() — compile-time field linkage
    ├── Layer 2: contract tests — runtime behavioral checks
    │
    ▼
Ghost model (SubscriberBufferGhost)    ← Pure Rust, no Verus
    │  bool has_data, overflow, locked
    │  usize stored_len, buf_capacity
    │
    ├── Verus spec functions (callback_post_fix, try_recv_post_fix, etc.)
    │
    ▼
Verus proofs (e2e.rs)
    • no_stuck_subscription
    • no_silent_truncation
    • lock_prevents_data_race
    • process_in_place_clears_correctly
```

**Strengths:**
- Simple. Proofs are ~20 lines each, discharged instantly by Z3.
- Ghost types live in a plain Rust crate (no Verus dependency).
- Kani can also reason about the same ghost types (160 harnesses).
- Works with `no_std` production code that uses atomics (which Verus can't
  import directly).

**Weaknesses:**
- The ghost model is **manually written** and validated by tests, not by the
  type system. A logic change in production that slips past contract tests
  would leave the ghost model stale.
- The proofs reason about **sequential** state transitions, not true
  concurrency. `lock_prevents_data_race` (proof 15) manually sequences
  operations rather than proving interleaving safety.
- No ownership transfer — the proof can't say "only the producer can write
  `has_data`" in a machine-checkable way.

### 2.2 What a tokenized approach would look like

```
tokenized_state_machine!{SubscriberProtocol {
    fields {
        #[sharding(constant)]
        pub buf_capacity: usize,

        #[sharding(storage_option)]
        pub data_permission: Option<DataPermission>,  // permission to read/write buf

        #[sharding(variable)]
        pub has_data: bool,        // tracks the AtomicBool

        #[sharding(variable)]
        pub overflow: bool,        // tracks the AtomicBool

        #[sharding(variable)]
        pub locked: bool,          // tracks the AtomicBool

        #[sharding(variable)]
        pub stored_len: usize,     // tracks the AtomicUsize

        #[sharding(variable)]
        pub writer_state: WriterState,   // callback-local

        #[sharding(variable)]
        pub reader_state: ReaderState,   // executor-local
    }
```

Transitions would be:
- `callback_start`: writer checks `locked`, checks capacity, withdraws `data_permission`
- `callback_end`: writer deposits initialized `data_permission`, sets `has_data`
- `read_start`: reader sets `locked`, withdraws `data_permission`
- `read_end`: reader deposits consumed `data_permission`, clears `has_data` + `locked`

**The ownership transfer** is the key improvement: when the callback holds
`data_permission`, the reader provably cannot access the buffer. When the
reader holds it (locked=true), the callback provably cannot write.

### 2.3 Comparison

| Aspect | Current (ghost model) | Tokenized SM |
|---|---|---|
| **Concurrency proof** | Sequential steps, manual | Interleaving-safe, automatic |
| **Ownership tracking** | Not tracked | Machine-checked via tokens |
| **Ghost–production coupling** | 3-layer tests | Direct (tracked fields in production types) |
| **Proof complexity** | ~20 lines per proof | ~100+ lines per proof |
| **Dependencies** | Plain Rust crate | Requires `vstd`, `PCell`, `AtomicU64` ghost API |
| **no_std compatibility** | Full (ghost types are plain Rust) | PCell/AtomicU64 require vstd (std-only currently) |
| **Kani compatibility** | Yes (shared ghost types) | No (tokenized SM is Verus-only) |

## 3. Feasibility Assessment for nano-ros

### 3.1 Blockers

**Blocker 1: `no_std` and atomics.** The production `SubscriberBuffer` uses
`portable_atomic::AtomicBool` and `portable_atomic::AtomicUsize` for embedded
targets. Verus's `AtomicU64` is a `vstd` type backed by `std::sync::atomic`.
To use tokenized state machines, we'd need either:
  - (a) A Verus-compatible atomic abstraction for `no_std` (doesn't exist), or
  - (b) Wrapper types that use `portable_atomic` in production and `vstd::atomic`
    in verification mode (`cfg(verus_keep_ghost)`).

**Blocker 2: Static allocation.** The subscriber buffers are statically allocated
(`static SUBSCRIBER_BUFFERS: [SubscriberBuffer; N]`). Verus's `PCell` is
heap-allocated (`Vec<PCell<T>>`). For embedded targets, we need static PCells or
an equivalent — this would require extending vstd.

**Blocker 3: ISR context.** The callback (producer) runs in ISR or zenoh-pico
thread context. Verus's `atomic_with_ghost!` macro assumes standard Rust threads
with `spawn`. ISR preemption has different semantics (not preemptible by same-priority
interrupts, but preemptible by higher-priority). The state machine model would need
to account for ISR-specific constraints.

**Blocker 4: FFI boundary.** The callback is invoked from C code (zenoh-pico).
Verus can't verify the C caller. The `tokenized_state_machine!` transitions would
need to be callable from `extern "C"` functions, which means the ghost token
passing happens at the FFI boundary — a trust boundary.

### 3.2 What IS feasible

Despite the blockers on full integration, a tokenized state machine can still
provide value as a **refinement specification**:

1. **Define the protocol** as a `tokenized_state_machine!` that models the
   concurrent access pattern (callback vs. executor).
2. **Prove invariant preservation** for all transition interleavings.
3. **Keep the ghost-model approach** for linking to production code, but add a
   **refinement lemma** that proves the ghost model transitions correspond
   1-to-1 with tokenized SM transitions.

This gives us:
- Machine-checked concurrency safety (from the tokenized SM)
- Practical production coupling (from the ghost model + 3-layer validation)
- No production code changes required

### 3.3 Sketch of the refinement approach

```
┌─────────────────────────────────────────┐
│ tokenized_state_machine!(SubscriberSM)  │  ← Concurrency proof
│   • Proves: no data race                │
│   • Proves: lock semantics correct      │
│   • Proves: ownership transfer sound    │
└────────────────┬────────────────────────┘
                 │ refinement lemma
                 │ (each ghost-model transition
                 │  corresponds to an SM transition)
                 ▼
┌─────────────────────────────────────────┐
│ SubscriberBufferGhost                   │  ← Sequential spec
│   • callback_write()                    │
│   • try_recv_raw()                      │
│   • process_in_place()                  │
└────────────────┬────────────────────────┘
                 │ 3-layer validation
                 │ (compile-time + contract tests)
                 ▼
┌─────────────────────────────────────────┐
│ SubscriberBuffer (production)           │  ← Actual code
│   • AtomicBool has_data, overflow       │
│   • subscriber_notify_callback()        │
│   • try_recv_raw()                      │
└─────────────────────────────────────────┘
```

The refinement lemma would look like:

```rust
proof fn callback_refines(
    pre: SubscriberSM::State, msg_len: usize,
    ghost_pre: SubscriberBufferGhost,
)
    requires
        states_correspond(pre, ghost_pre),
        !pre.locked,
    ensures
        ({
            // SM transition
            let post = SubscriberSM::State::callback_start_end(pre, msg_len);
            // Ghost model transition
            let ghost_post_result = ghost_pre.callback_write(msg_len);
            // They agree
            states_correspond(post, ghost_post_result.new_state)
        }),
{ }
```

## 4. Recommendation

### Short term: Keep the ghost-model approach, strengthen Layer 2

The current approach is well-suited to nano-ros's constraints (no_std, static
allocation, ISR context, FFI boundary). The tokenized state machine blockers
are significant and would require upstream Verus changes.

**Concrete improvements:**
1. Add more contract tests that exercise **interleaved** operations (e.g.,
   callback during `try_recv` lock window).
2. Add Kani harnesses that check all 2-step interleavings of callback and
   read operations on the ghost model.
3. Document the sequential-proof limitation explicitly.

### Medium term: Add a tokenized SM as a refinement spec

Write the `SubscriberSM` as a tokenized state machine in `nros-verification`
that models the concurrent protocol. Prove invariant preservation. Add a
refinement lemma linking it to `SubscriberBufferGhost`. This gives
machine-checked concurrency reasoning without changing production code.

**Effort estimate:** ~300 lines of Verus (state machine + invariants +
inductive proofs + refinement lemmas). This is feasible with the current
Verus toolchain since it doesn't require `PCell` or `AtomicU64` integration —
the tokenized SM would be a pure specification, not linked to runtime types.

### Long term: Full integration (blocked on Verus no_std support)

If Verus adds `no_std`-compatible `PCell` and atomic ghost state, the full
integration becomes feasible. The production `SubscriberBuffer` would carry
`Tracked` tokens, and each atomic operation would use `atomic_with_ghost!`.
This would eliminate the ghost-model gap entirely.

**Prerequisites:**
- vstd `PCell` for static allocation (`no_std`)
- vstd atomic ghost state for `portable_atomic` types
- Verus support for `extern "C"` functions with tracked parameters

## 5. Applicability to Other nano-ros Components

### 5.1 CDR serialization (CdrWriter/CdrReader)

**Not a good fit** for tokenized SM. CDR serialization is single-threaded
(no concurrency). The current ghost model approach (CdrGhost + field linkage)
is sufficient. Tokenized SM would add complexity without benefit.

### 5.2 Executor spin loop (SpinOnceGhost)

**Moderate fit.** The executor's `spin_once()` processes subscriptions and
services in a loop. There's no concurrency within `spin_once`, but the
interaction between `spin_once` and callbacks IS concurrent. A tokenized SM
could model the executor↔callback protocol:
- `spin_start`: executor acquires all subscription locks
- `process_sub`: executor processes one subscription (reads + unlocks)
- `spin_end`: executor completes
- `callback_fire`: callback fires on a subscription (concurrent with spin)

However, the executor doesn't actually lock all subscriptions at once — it
processes them one at a time. So the concurrency is per-subscription, which
is already modeled by the SubscriberBuffer protocol.

### 5.3 Service buffer (ServiceBufferGhost)

**Good fit** — same concurrent access pattern as SubscriberBuffer. If we build
a tokenized SM for the subscriber buffer, the service buffer can reuse the
same pattern.

### 5.4 Ring buffer (proposed in message-delivery-verification.md)

**Best fit.** If the subscriber buffer evolves from depth-1 (current) to a
ring buffer (proposed in the message delivery guarantees design), the SPSC
ring buffer is the canonical tokenized state machine example. The Verus FIFO
queue can be adapted almost directly:
- `backing_cells` → ring buffer slots
- `storage_map` → per-slot `PointsTo` permissions
- `produce_start/end` → callback writes into next slot
- `consume_start/end` → executor reads from oldest slot

This is where tokenized state machines provide the most value — they
eliminate the entire class of ring buffer bugs (ABA, double-read, write-to-occupied)
by construction.

## 6. Summary

| | Current ghost model | Tokenized SM (refinement) | Tokenized SM (full) |
|---|---|---|---|
| **Concurrency safety** | Manual (contract tests) | Machine-checked (spec-only) | Machine-checked (end-to-end) |
| **Production coupling** | 3-layer validation | Ghost model + refinement | Direct (tracked fields) |
| **Effort** | Done | ~300 lines Verus | ~600+ lines + Verus upstream |
| **Blockers** | None | None | no_std PCell, ISR atomics |
| **Value** | Correctness of sequential state machine | + interleaving safety of protocol | + elimination of ghost-model gap |
