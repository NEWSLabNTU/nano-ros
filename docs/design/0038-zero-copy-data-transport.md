---
rfc: 0038
title: "Zero-copy data transport — shared slot pool + in-place receive dispatch"
status: Draft
since: 2026-06
last-reviewed: 2026-06
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# RFC-0038 — Zero-copy data transport: shared slot pool + in-place receive dispatch

## Summary

The default subscription receive path copies every message **twice** before it
reaches user code, and pre-allocates the receive buffers as **per-subscriber
fixed arrays** that do not scale to large messages. This RFC records the design
for collapsing the redundant copy and the redundant buffer:

1. **Dispatch user callbacks in-place** from the RMW backend's receive ring slot
   (the existing `Subscriber::process_raw_in_place` borrow), deleting the
   executor's second buffering layer (the arena `BufferStrategy`). This removes
   **copy #1** (ring → arena) and the arena receive buffer.
2. **Replace per-subscriber fixed rings** with **one shared slot pool** drawn
   per-subscription up to its runtime QoS depth (drop-oldest), so static RAM is
   `MAX_HISTORY × slot` (shared) instead of `MAX_SUBS × DEPTH × slot` (per-sub).
   This is the scaling fix for large messages.

The lifetime guard is the backend's slot-ownership protocol: the consumer
(executor) controls slot release (after the callback returns); the producer never
overwrites a borrowed slot. On a true SPSC backend (zpico) this needs no extra
lock; multi-producer backends guard only slot (de)allocation, not the borrow
(D3). Copy #2 (CDR deserialize into the message struct) is out of scope here — it
is removed for borrowed `&'a` message types by the borrowed-deserialization
codegen of **issue #7 / RFC-0033**, using the same borrow window this RFC opens.

**Interface minimality.** The change crosses the RMW boundary as exactly **two
`Subscriber` trait methods** — `process_raw_in_place` (already present) and
`process_raw_in_place_with_info` (added) — plus **one optional** cffi vtable slot.
No QoS, pool, or depth concept enters the trait; the shared pool is
backend-internal and optional, reached only through those methods. The design is
**transport-agnostic**: the link (TCP / UDP / serial / Bluetooth / CAN / custom
`NrosTransportOps`) is configured below the RMW boundary, and switching links
does not touch the receive path (see *Transport portability*).

This is the design-of-record for **issue #8** (two-copy receive + static
pre-allocation at scale). It is **"Form B"** from the issue #8 design discussion:
a true single-copy data plane (network → slot → user), chosen over "Form A"
(drop the arena, accept one global compile-time ring depth) because Form A cannot
honor per-subscription ROS QoS history depth.

## Motivation / problem

Every subscription message traverses two copies
(`docs/issues/0008-two-copy-receive.md`):

```
Network → zpico SUBSCRIBER_BUFFERS ring → arena BufferStrategy slot → message struct
              (zenoh C write, no copy)      (copy #1: try_recv_raw memcpy)  (copy #2: CDR)
```

- **Copy #1** is `try_recv_raw` copying the backend ring slot into the executor
  arena buffer (`packages/zpico/nros-rmw-zenoh/src/shim/subscriber.rs:944`
  `buf[..len].copy_from_slice(&buffer.ring_payload[slot][..len])`), consumed by
  the arena dispatch (`packages/core/nros-node/src/executor/arena.rs:290`
  `drain_into_buffer` → arena.rs:318 `sub_buffered_try_process`).
- **Copy #2** is CDR deserialization from the arena slot into the message struct
  (arena.rs:337).

The arena `BufferStrategy` (arena.rs:225–261; `Triple` for `KEEP_LAST(1)`, `Ring`
for `KEEP_LAST(N>1)`) is a **second** decoupling buffer in series behind the
backend's own SPSC ring (`SUBSCRIBER_RING_DEPTH`, default 4). For the common case
it is pure overhead: copy #1 + extra static RAM.

The dominant cost is **static pre-allocation at scale**. zpico sizes the receive
ring as a per-subscriber fixed array:

```
SUBSCRIBER_BUFFERS: [SubscriberBuffer; ZPICO_MAX_SUBSCRIBERS]   // default 8 subs
  └ ring_payload: [[u8; SUBSCRIBER_BUFFER_SIZE]; SUBSCRIBER_RING_DEPTH]  // 1 KB × 4
```

≈ 32 KB by default. Raise `ZPICO_SUBSCRIBER_BUFFER_SIZE` to 64 KB for compressed
images and it explodes to `8 × 4 × 64 KB = 2 MB` — impossible on any MCU —
regardless of how many subscribers actually exist or how deep their QoS is.

The two goals are linked: removing the arena layer (copy #1) only pays off if the
*one remaining* buffer is sized sanely, which means moving QoS depth off a
compile-time per-subscriber array dimension and onto a runtime budget against a
shared pool.

## Background — what already exists

- `Subscriber::process_raw_in_place(f: impl FnOnce(&[u8]))`
  (`packages/core/nros-rmw/src/traits.rs:1440`) is a true zero-copy borrow:
  zpico implements it (subscriber.rs:1043) as
  `f(&buffer.ring_payload[slot][..len]); buffer.consume_head();` — borrow the
  ring slot, run `f`, release. It is wired only into the **manual-poll handle**
  (`packages/core/nros-node/src/executor/handles.rs:1187`), **not** the arena
  executor loop. The default trait body returns `Err(MessageTooLarge)`; only
  zpico overrides it (xrce / cffi / cyclonedds / mock fall back).
- The opt-in `lending` feature (`SlotBorrowing` / `ZenohView`) and the
  alloc-only `unstable-zenoh-api` path already remove copy #1 on narrow paths.
  Neither is the default receive path.
- QoS depth (`QosSettings::depth`, traits.rs:372) currently sizes the arena
  buffer only (`buffered_region_size(qos.depth, ...)`, arena.rs:250). It does
  **not** reach the backend ring, whose depth is the build-time
  `SUBSCRIBER_RING_DEPTH` constant.

## Reference survey

Three production middlewares, studied under `external/` (read-only):

### rmw_zenoh_cpp (same transport, desktop)

Holds the Zenoh-owned payload (ref-counted `zenoh::Bytes`) in a per-subscription
`std::deque` **sized by QoS depth**, drop-oldest on overflow
(`rmw_subscription_data.cpp:504`). `rmw_take` deserializes in-place via a FastCDR
view over the payload, then releases it (`rmw_subscription_data.cpp:383`). The
borrow window is the deserialize call, under the per-subscription mutex.
**Loaned messages: not supported** (`can_loan_messages = false`).

### DDS — rmw_cyclonedds / rmw_fastrtps (canonical loan model)

The ROS loaned-message API (`external/rmw/rmw/include/rmw/rmw.h`):
`rmw_take_loaned_message` returns a **middleware-owned** buffer the caller borrows
until `rmw_return_loaned_message_from_subscription`. Key gates that **port to an
embedded no-alloc backend**:

- **`is_plain` / `is_self_contained` gate** — only fixed-size, pointer-free types
  loan; variable-length types fall back to a copy. (cyclonedds rmw_node.cpp:2488;
  fastrtps rmw_take.cpp:472.)
- **Pin a history-cache slot during the borrow** — the loaned pointer holds one of
  N reader-cache slots until returned; bounded loan pool (fastrtps `LoanManager`,
  rmw_take.cpp:422).
- The **borrow-then-return lifetime contract** itself is alloc-free.

Not portable: the true zero-copy variant needs shared memory (iceoryx) /
data-sharing QoS — desktop-only.

### micro-XRCE-DDS (the production MCU peer — most relevant)

Keeps **two copies** on purpose: transport stream → a **shared static buffer
pool** (`custom_static_buffers[RMW_UXRCE_MAX_HISTORY]`, default 8, shared across
**all** subscriptions/services) → user message via micro-CDR at take
(`callbacks.c` `on_topic`; `rmw_take.c`). It deliberately does **not** borrow the
transport frame: the stream buffer's lifetime is tied to the RX cycle, unsafe
across an async user callback. QoS `KEEP_LAST(N)` draws up to N slots from the
shared pool per entity, **drop-oldest by timestamp** (`types.c:239`). No loaned
messages.

### What the survey decides

- micro-XRCE confirms an embedded middleware needs **one** decoupling buffer
  between the RX cycle and the user callback — but **nano-ros already has it**:
  the zpico SPSC ring is exactly micro-XRCE's static pool. Our copy #1 is *ring →
  a second ring*, which micro-XRCE does **not** have. So the micro-XRCE
  "don't borrow the transport frame" caution does **not** forbid our change: we
  borrow the **decoupling ring slot**, not the raw transport frame.
- micro-XRCE's **shared pool** (runtime per-entity depth, static total) is the
  model that resolves the static-array-vs-runtime-QoS tension. ROS-QoS-respecting
  via drop-oldest, matching rmw_zenoh and DDS `KEEP_LAST`.
- DDS's **is_plain gate** + **slot-pin-during-borrow** are the portable parts of
  the loan model; they apply to copy #2 elimination for borrowed types (issue #7).

## RMW interface surface — what crosses the boundary (and what does not)

The executor must not learn anything about a backend's buffer model. It
references **zero** backend ring internals today (no `ring_payload` /
`SUBSCRIBER_BUFFERS` in `nros-node`); it goes through the `Subscriber` trait
only. This RFC keeps it that way. The **entire** interface Form B needs is two
trait methods:

| Method | Status | Contract |
| --- | --- | --- |
| `process_raw_in_place(f: FnOnce(&[u8])) -> Result<bool>` | **exists** (traits.rs:1440) | Borrow one ready message as a **contiguous** CDR slice for the closure; release after. `Ok(false)` = none ready. |
| `process_raw_in_place_with_info(f: FnOnce(&[u8], &MessageInfo)) -> Result<bool>` | **must add to trait** | Same, plus the co-located attachment (GID / seq / source-stamp). Currently a zpico *inherent* method (subscriber.rs:871); promote it to the trait. |

That is the whole interface delta — two methods, one already present. **No QoS,
pool, depth, or slot concept appears in the trait**: QoS depth is already passed
at subscription creation (`QosSettings::depth`) and is handled entirely
backend-side. Everything in **D1 below is backend-internal and optional** — a
zpico-and-friends storage refactor, not a cross-backend mandate. A backend that
never adopts the shared pool, or never implements the in-place methods, keeps
working via the buffered fallback (C2). This is the anti-bloat guarantee: the
boundary grows by one method, and the default-NULL/`Err(unsupported)` contract
(RFC-0035) lets each backend opt in independently.

The cffi C ABI grows by **one optional vtable slot** for the in-place take
(append-to-tail per RFC-0035); NULL → the runtime uses the buffered fallback. No
backend is forced to populate it.

## Design — Form B (refined)

```
Network → [shared slot pool] → in-place deserialize + user callback → release slot
          (transport writes here)  (borrow window = callback scope)
```

The data plane is **backend-internal**; the executor sees only the two trait
methods above. D1 (pool) and D2 (dispatch loop) are described together for
clarity, but D1 is per-backend and optional while D2 is the one executor change.

### D1. Shared receive slot pool (replaces per-sub fixed rings)

Replace the per-subscriber fixed ring
(`[[u8; SIZE]; DEPTH]` × `MAX_SUBSCRIBERS`) with a single backend-owned pool of
`MAX_HISTORY` slots of `SLOT_SIZE` bytes, plus a parallel attachment slot array.
Each slot carries `{ len, owner, timestamp, attachment }`. A subscription draws
slots from the pool up to its **runtime QoS depth**; on overflow it recycles its
own oldest slot (drop-oldest by timestamp). Static RAM becomes
`MAX_HISTORY × SLOT_SIZE` (shared) regardless of subscriber count or per-sub
depth. `MAX_HISTORY`, `SLOT_SIZE`, `MAX_SUBSCRIBERS` stay build-time knobs
(env → generated consts, as today), but the *per-sub* depth is runtime.

This mirrors micro-XRCE `custom_static_buffers` and rmw_zenoh's per-sub deque,
fused into one shared allocation.

### D2. In-place arena dispatch (removes copy #1 + the arena buffer)

Delete `BufferStrategy` (`Triple`/`Ring`) from the subscription arena entries
(`SubBufferedEntry`, `SubBufferedRawEntry`, arena.rs:276/384). Replace
`drain_into_buffer` + buffered dispatch with a loop over the backend's pending
slots via `process_raw_in_place`:

```text
while sub.process_raw_in_place(|raw| {
    // typed:    deserialize(raw) -> msg; callback(&msg)      (copy #2 stays)
    // borrowed: callback(raw)  // &'a msg borrows raw         (copy #2 gone)
}) {}
```

The per-subscription arena trailing buffer shrinks to zero. The executor entry
holds only the handle + callback. QoS depth no longer sizes any arena region; it
is passed to the backend as the per-sub slot budget (D1).

### D3. Lifetime model

The borrow window is the `process_raw_in_place` closure = the callback scope. The
slot-ownership protocol is the lifetime guard:

- Consumer (executor) holds the slot for the callback's duration, then advances
  the consume cursor (`consume_head`) — *the slot is released only after the
  callback returns*.
- The producer never touches a slot the consumer is borrowing; on a full pool it
  **blocks or drops-oldest** (per QoS) rather than overwrite a live slot.

The cost of the slot guard is **backend-shaped**, and this is where the
generic "no new lock" claim must be qualified:

- **True SPSC backend (zpico today):** one transport thread, one executor
  consumer, head/tail cursors. The borrow needs **no extra lock** — the cursor
  protocol alone is the guard.
- **Multi-producer / multi-consumer backend (e.g. DDS reader threads, or a
  multi-tier executor consuming one pool):** the shared-pool **slot allocator**
  (which sub owns which slot, claim/release) needs a short critical section or
  atomic free-list at the producer/consumer boundary regardless — see C2. The
  *borrow itself* is still lock-free; only slot (de)allocation is guarded.
- **Single-threaded polled backend (no producer thread — see Transport
  portability):** there is no concurrent producer at all, so the borrow window
  cannot stall anything; back-pressure is moot.

### D4. Copy #2 and borrowed types (scope boundary)

Copy #2 (CDR field-by-field into the message struct) is **not** removed by this
RFC for owned message types — deserialization inherently writes the struct. It
**is** removed for **borrowed `&'a` message types** (e.g. `Image<'a>`), whose
`deserialize_borrowed` slices directly out of the borrowed slot inside the same
D2 window. The `is_plain` gate (DDS) decides per type whether the borrowed path
is available. The borrowed-deserialization codegen is **issue #7 / RFC-0033**
(owned / heap / borrowed modes); this RFC provides the borrow window it needs.

## Transport portability — network, serial, and other links

The in-place design sits **above** the link layer and is therefore
transport-agnostic, but only because the backend upholds two obligations. The
link (TCP / UDP / serial / Bluetooth / raw-eth / CAN) is configured **below** the
RMW boundary — zenoh-pico `Z_FEATURE_LINK_{TCP,SERIAL,BLUETOOTH,WS,RAWETH}`,
micro-XRCE UART/UDP transports, the runtime-pluggable `NrosTransportOps`
(`nros-rmw-cffi/include/nros/rmw_transport.h`). The `Subscriber` never sees the
link; it sees a reassembled message in a slot. For that to hold:

### T1. Linearization is the backend's job (the one unavoidable copy)

`process_raw_in_place(&[u8])` requires a **contiguous** CDR slice. Links that
deliver fragments — serial/CAN frame streams, scatter-gather datagrams, zenoh's
non-contiguous `Bytes` — must **reassemble/linearize into one slot before
exposing the slice**. That linearization is the unavoidable *network → slot*
write (the same copy micro-XRCE keeps; rmw_zenoh_cpp's `as_vector()` for the
non-contiguous case). It is **distinct from copy #1**: copy #1 was *slot → a
second slot*, which Form B removes; the linearization into the first slot
remains and is not "a copy we eliminate." The contract is: **the backend owns
exactly one linearization into the pool slot, then dispatches in-place from it.**
Backends whose link is already message-framed and contiguous (zenoh-pico over
TCP into its ring) do this for free.

### T2. Slot size is bounded by the message, not the MTU

On small-MTU links (serial, CAN, BLE), a ROS message spans many frames. The pool
`SLOT_SIZE` must hold the **largest reassembled message**, decoupled from the
link MTU (micro-XRCE sizes its input buffer `MTU × STREAM_HISTORY`). Reassembly
staging (the partial-message buffer) is a per-backend concern below the pool and
is **not** part of this RFC's slot accounting — the pool holds completed
messages only.

### T3. Polled vs threaded links drive who pumps and whether stall applies

- **Threaded link (RTOS network thread, zenoh-pico RX task, RTOS serial ISR +
  task):** a producer thread linearizes into pool slots asynchronously;
  `process_raw_in_place` dispatches what is staged. D3's back-pressure applies.
- **Single-threaded polled link (bare-metal serial/UDP, no RTOS threads):** there
  is no producer thread. `process_raw_in_place` must itself **pump the
  transport** (read + linearize one message) before dispatching, or a sibling
  `spin`-time drain must. The borrow cannot stall a producer (there is none);
  the cost is simply that receive progresses only while the executor spins —
  already true for the polled execution model. The trait contract is therefore
  *"advance the link if needed, then borrow-dispatch at most one message,"* which
  both the threaded and polled backends satisfy.

### T4. The link is swappable without touching the receive path

Because the pool + in-place dispatch live above `NrosTransportOps` /
`Z_FEATURE_LINK_*`, switching a deployment from network to serial (or adding a
custom link) changes only the transport config, **not** the subscription receive
path, the executor, or the QoS-depth handling. This is the portability payoff:
one receive design across all links.

## Consequences

- **Producer stall (the central tradeoff — threaded links only).** On a threaded
  link a callback now runs while holding a pool slot, so a slow callback
  back-pressures the transport thread (pool fills → producer blocks or
  drops-oldest). Today the arena copy decouples them. Mitigation: the pool depth
  *is* the decoupling budget (a slow consumer tolerates `budget` in-flight
  messages before drops), and the executor already bounds callback work per spin.
  Documented as a QoS-tuning knob, not hidden. On single-threaded polled links
  (T3) there is no producer to stall.
- **Slot allocator cost.** The shared pool needs a per-slot `owner`/`free` state
  updated by the producer (claim a slot) and consumer (release). On
  multi-threaded RTOS this is a short critical section or an atomic free-list —
  one per message, off the payload-copy path. Bounded and cheap vs the eliminated
  memcpy.
- **Backend coverage (incremental, no flag-day).** Only zpico implements
  `process_raw_in_place`. xrce, cffi (vtable/opaque), cyclonedds, and the mock
  fall back to the trait default. The buffered dispatch is **retained as a
  fallback** invoked when the in-place method returns the unsupported error, so
  each backend opts in independently — zpico first, the rest as follow-ups, no
  flag-day. cffi gets **one optional** in-place vtable slot (append-to-tail per
  RFC-0035); **NULL → buffered fallback** per the 0035 NULL contract, so no
  backend is forced to populate it. The `process_raw_in_place_with_info` trait
  method (attachment) is added alongside, defaulting to the same unsupported
  error so existing backends compile untouched.
- **Latest-value (`KEEP_LAST(1)`) semantics.** The `Triple` buffer gave
  drop-old/keep-newest. The shared pool's drop-oldest-by-timestamp reproduces
  KEEP_LAST(N) FIFO; KEEP_LAST(1) collapses to a single slot recycled per
  message, which is the same observable latest-value behavior for a
  single-consumer executor.
- **Attachment / metadata.** The attachment ring must move with the payload slot
  (co-located `{payload, attachment}` per pool slot), so
  `try_recv_raw_with_info` / `process_raw_in_place_with_info` borrow both together.
- **Tests pin the current contract.** `triple_buffer.rs`, `spsc_ring.rs`, and the
  QoS-depth arena tests assume the two-ring shape and must be reworked to the
  pool model. The `lending` feature tests and the safety-e2e in-place tests
  already exercise the borrow path and become load-bearing.

## Per-backend plan

| Backend | `process_raw_in_place` today | Plan |
| --- | --- | --- |
| zpico (zenoh-pico) | implemented (subscriber.rs:1043) | shared pool (D1) + arena dispatch (D2); primary target |
| xrce (micro-XRCE) | falls back | shared pool already its native model; add in-place take or keep buffered fallback |
| cyclonedds | falls back | buffered fallback first; native loan path is a later follow-up |
| cffi (C ABI) | falls back | add an append-to-tail in-place vtable slot (RFC-0035); else buffered fallback |
| mock | falls back | buffered fallback (test-only) |

Land zpico first behind the fallback (C2b) so the other backends are unaffected
until individually migrated.

## Alternatives considered

- **Form A — drop the arena, one global compile-time ring depth.** Dispatch
  in-place straight from the backend ring, map QoS depth onto the build-time
  `SUBSCRIBER_RING_DEPTH`. Simplest, removes copy #1 and the arena RAM. **Rejected:**
  every subscription shares one compile-time depth, so per-subscription ROS QoS
  history (`KEEP_LAST(N)`) is not honored — a correctness regression against ROS
  semantics. Form B keeps per-sub depth via the runtime pool budget.
- **Status quo (two-copy + opt-in `lending`).** Keep the default two-copy path;
  users who need zero-copy enable `lending`. **Rejected:** the static
  pre-allocation explosion (the 2 MB image case) is in the *default* buffer
  sizing, not the copy — opt-in lending does not fix the RAM scaling.
- **Full loaned-message API (`rmw_take_loaned_message` surface).** Expose the ROS
  loan API to users. Deferred: neither rmw_zenoh nor micro-XRCE expose it on
  these transports; our in-place dispatch achieves the same data-plane win
  internally without the user-facing borrow/return contract. Revisit if a
  user-facing loaned-message API is wanted.

## Open questions

1. **Pool sizing default.** `MAX_HISTORY` and `SLOT_SIZE` defaults that balance
   small-message subscriber count vs large-message depth. micro-XRCE uses 8 total
   slots; is a single `SLOT_SIZE` right, or should the pool be tiered by size
   class (small vs large slots)?
2. **Fallback vs full migration.** Ship C2b (buffered fallback for non-zpico) and
   migrate backends lazily, or block on all-backend in-place support?
3. **cffi in-place ABI shape.** What does the in-place take slot look like across
   the C ABI — a callback-taking `process_raw_in_place(ctx, fn)` slot, vs a
   borrow/return slot pair? (RFC-0035 append-to-tail.)
4. **Producer back-pressure policy.** Block the transport thread vs drop-oldest
   when a callback holds the pool full — per-QoS (RELIABLE vs BEST_EFFORT)?
5. **Interaction with multi-tier executors (Phase 228).** A pool slot borrowed by
   a callback on one tier while another tier's transport thread produces — the
   slot allocator guard must be tier-safe (it already must be thread-safe).
6. **Polled-pump contract (T3).** Should `process_raw_in_place` itself pump a
   polled link, or should a separate `spin`-time drain step own the
   read+linearize so the method stays "dispatch only"? Affects single-threaded
   bare-metal serial/UDP backends and the method's documented contract.
7. **Linearization for scatter-gather backends (T1).** For a backend whose link
   delivers non-contiguous payloads (zenoh `Bytes`, fragmented datagrams), is the
   one-linearization-into-slot acceptable as the backend's responsibility, or
   should the trait expose a chunked/segmented in-place variant to avoid even
   that copy for very large messages? (Defaults to "backend linearizes.")

## Relationship to other work

- **issue #8** (`docs/issues/0008-two-copy-receive.md`) — this RFC is its
  design-of-record; issue #8 flips to point here.
- **issue #7 / RFC-0033** — copy #2 elimination for borrowed types uses D2's
  borrow window; the `is_plain` gate decides availability per type.
- **RFC-0035** (RMW vtable ABI) — a cffi in-place take slot is an append-to-tail
  ABI change governed by 0035's evolution rule.
- **RFC-0006** — C-ABI-is-canonical; the in-place path must be expressible across
  the vtable, not Rust-only.
- A `docs/roadmap/` phase doc will carry the work items + acceptance tests and
  name this RFC in its `Implements:` header (per the RFC → roadmap → code flow).

## Acceptance (for the implementing phase)

- Default subscription receive on zpico performs **one** data-plane copy
  (transport → pool slot), with owned-message CDR as the only remaining copy and
  borrowed-message dispatch as zero data-plane copy.
- Static receive RAM is `MAX_HISTORY × SLOT_SIZE` (shared), independent of
  subscriber count and per-sub QoS depth; the 64 KB-image config no longer scales
  with `MAX_SUBSCRIBERS × DEPTH`.
- Per-subscription `KEEP_LAST(N)` is honored (N-deep FIFO, drop-oldest) from the
  shared pool.
- Non-zpico backends keep working via buffered fallback; no regression.
- `just ci` green; the existing `lending` / safety-e2e in-place tests pass
  against the new default dispatch.
