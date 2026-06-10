---
rfc: 0038
title: "Zero-copy data transport — shared slot pool + in-place receive dispatch"
status: Stable
since: 2026-06
last-reviewed: 2026-06
implements-tracked-by: [phase-231]
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
2. **Replace per-subscriber fixed rings** with **size-class slot pools**
   (`small` / `large`) drawn per-subscription up to its runtime QoS depth
   (drop-oldest), so static RAM is `Σ_class (N × slot_size)` (shared within a
   class) instead of `MAX_SUBS × DEPTH × largest_slot` (per-sub). The per-sub
   depth budget isolates subscribers; pools sized `≥ Σ(depths)` guarantee no
   cross-sub starvation. This is the scaling fix for large messages.

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

### Routing reality — the CFFI vtable is the activation gate

A correction to an earlier framing of this RFC ("zpico first, cffi later"): the
executor's `ConcreteSession` is **always** `CffiSession` or, in tests,
`MockSession` (`nros-node/src/session.rs:11/13`). The executor **never holds a
Rust `ZenohSubscriber` directly** — on native *and* embedded, zenoh-pico is
reached through the **CFFI vtable** (`CffiSubscriber`). Therefore:

- The arena consults `supports_process_in_place()` / `process_raw_in_place` on
  the **`CffiSubscriber`**, not on `ZenohSubscriber`. zpico's Rust
  `process_raw_in_place` is the **leaf** the vtable slot invokes, not a
  subscriber the executor selects.
- So the **CFFI in-place vtable slot is the activation gate** for *any* in-place
  dispatch through the executor — it is on the **critical path**, not an optional
  follow-up. Until `CffiSubscriber` forwards `process_raw_in_place` to a non-NULL
  vtable slot (and returns `true` from `supports_process_in_place`), the
  executor's in-place selection stays dormant and every subscription uses the
  buffered fallback (which is exactly why landing the executor scaffold first is
  safe and regression-free).
- The only backend the executor can hold **without** going through CFFI is
  `MockSubscriber` (tests). It keeps the buffered fallback.

Net: the activation order is (1) executor scaffold [done], (2) **CFFI vtable slot
+ `CffiSubscriber` forwarding + zenoh-pico C backend impl** [the activation], then
(3) the size-class pools (D1) underneath. The anti-bloat contract is unchanged —
the boundary still grows by two trait methods plus one optional vtable slot.

## Design — Form B (refined)

```
Network → [size-class slot pool] → in-place deserialize + user callback → release slot
          (transport writes here)    (borrow window = callback scope)
          per-sub depth budget = isolation
```

The data plane is **backend-internal**; the executor sees only the two trait
methods above. D1 (pools) and D2 (dispatch loop) are described together for
clarity, but D1 is per-backend and optional while D2 is the one executor change.

### D1. Size-class slot pools with a per-subscription depth budget

Replace the per-subscriber fixed ring (`[[u8; SIZE]; DEPTH]` × `MAX_SUBSCRIBERS`)
with a small set of backend-owned **size-class pools**. Each pool is `N` slots of
a fixed `slot_size`, plus a parallel attachment slot array; each slot carries
`{ len, owner, timestamp, attachment }`.

- **Size classes (resolves Q1).** Two classes by default — `small` and `large` —
  split by a build-time threshold (e.g. 2 KB). A subscription is routed to a
  class at codegen by its **bounded max-serialized-size** (known from RFC-0033
  capacity config). This avoids forcing every slot to fit the largest message: a
  64-byte IMU subscriber draws from `small`, not from a 64 KB `large` slot. Slot
  sizes and per-class counts are build-time knobs; the class count stays at 2 by
  default (do **not** expand to a `#QoS × #size` matrix — see Q4).
- **Per-subscription depth budget (resolves Q4 / the isolation mechanism).**
  Within its class pool, a subscription draws slots up to its **runtime
  `KEEP_LAST(N)` depth** and no further. A flooding `KEEP_LAST(1)` best-effort
  sensor holds exactly one slot — it recycles *its own* oldest slot
  (drop-oldest), never reaching into another subscriber's budget. The depth
  budget — not a separate QoS pool — is the cross-subscriber isolation.
- **Sizing default: guaranteed.** Size each class pool `≥ Σ(depths of its member
  subscriptions)` so every subscription is guaranteed its depth and the only drop
  is the intended per-sub `KEEP_LAST(N)` recycle — **zero** cross-subscriber
  starvation. Codegen knows the membership and depths, so it can size the pools
  (or warn) at bake time.
- **Overcommit + reliable reservation: opt-in.** A tight-RAM deployment may size
  a pool **below** `Σ(depths)`; then subscribers compete and drop-oldest can
  evict across subscriptions. **Only in that overcommitted mode** does a reserved
  `reliable` sub-pool earn its keep (reserve slots for `RELIABLE` traffic so a
  best-effort flood cannot evict control messages). This is a documented opt-in
  knob, not the default, so the common case keeps the pool count low and the
  statistical-multiplexing benefit of sharing within a class.

Static RAM becomes `Σ_class (N_class × slot_size_class)` — independent of
subscriber count and bounded by the guaranteed/overcommit choice, not by
`MAX_SUBS × DEPTH × largest_slot`. The 64 KB-image config sizes only the `large`
pool's few slots.

This fuses micro-XRCE's shared static pool (runtime per-entity depth, static
total) with rmw_zenoh's per-sub `KEEP_LAST` budget, partitioned by size class.
**Backend-internal and optional**: a backend that keeps per-sub buffers still
works via the fallback (C2); the pools are a zpico-first storage refactor.

**Implementation note (Phase 231 Wave 2).** The zenoh-pico C producer
(`sample_handler`) is generic over the per-sub `zpico_ring_desc_t` descriptor, so
the first landing keeps the proven per-sub SPSC ring and splits the **static
storage into two size classes** (`small` / `large` per-sub rings) rather than a
single shared slot pool. This kills the `MAX_SUBS × DEPTH × large_slot` explosion
and gives full per-sub isolation, but RAM is
`Σ_class (MAX_class × DEPTH × slot_size)` — **not** fully sub-count-independent.
The *shared* slot pool (sub-count independence, drawing slots across subs via a
claim/release allocator that replaces the per-sub SPSC ring in the C producer) is
a **deferred refinement** (call it **Q-pool**), worth its C-side rewrite only for
deployments with many large subscribers. The size-class split delivers the
headline win at a fraction of the risk.

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
- **Backend coverage (incremental, gated on the CFFI slot).** Because the
  executor holds a `CffiSubscriber` (not `ZenohSubscriber`) for every non-mock
  backend, the **CFFI in-place vtable slot is the activation gate** — until it
  exists and `CffiSubscriber` forwards to it, the executor's in-place selection is
  dormant and everything uses the buffered fallback. The buffered dispatch is
  **retained as the fallback** invoked when `supports_process_in_place()` is
  false, so a NULL slot per backend (cyclonedds, an un-migrated xrce) keeps
  working with no flag-day. cffi gets **one optional** in-place vtable slot
  (append-to-tail per RFC-0035 + `abi_version` bump); **NULL → buffered fallback**
  per the 0035 NULL contract. The `process_raw_in_place_with_info` trait method
  (attachment) is added alongside, defaulting to the unsupported error so existing
  backends compile untouched. `MockSubscriber` (the only directly-held non-CFFI
  backend, test-only) keeps the buffered fallback.
- **Latest-value (`KEEP_LAST(1)`) semantics.** The `Triple` buffer gave
  drop-old/keep-newest. The per-sub depth budget with drop-oldest-by-timestamp
  reproduces KEEP_LAST(N) FIFO; KEEP_LAST(1) collapses to a single slot recycled
  per message, the same observable latest-value behavior for a single-consumer
  executor.
- **Back-pressure policy (resolved Q4).** Overflow **drops-oldest at the pool**,
  never blocks the shared transport thread (blocking it = head-of-line blocking
  across every subscription on that thread). RELIABLE is honored **upstream**
  (zenoh reliable channel / DDS history retransmit), not by stalling the consumer
  — matching rmw_zenoh and micro-XRCE. With pools sized `≥ Σ(depths)` the only
  drop is the intended per-sub `KEEP_LAST(N)`.
- **Attachment / metadata.** The attachment ring must move with the payload slot
  (co-located `{payload, attachment}` per pool slot), so
  `try_recv_raw_with_info` / `process_raw_in_place_with_info` borrow both together.
- **Tests pin the current contract.** `triple_buffer.rs`, `spsc_ring.rs`, and the
  QoS-depth arena tests assume the two-ring shape and must be reworked to the
  pool model. The `lending` feature tests and the safety-e2e in-place tests
  already exercise the borrow path and become load-bearing.

## Per-backend plan

The executor reaches every non-mock backend through **`CffiSubscriber`** (the
"Routing reality" section), so the layers are: the **CFFI vtable slot** (the
activation gate), the **C backend** that populates it, and the **Rust leaf** the C
backend calls.

| Layer | `process_raw_in_place` today | Plan |
| --- | --- | --- |
| executor / arena | selects via `supports_process_in_place()` | done (Wave 0.2) — dormant until `CffiSubscriber` says yes |
| `CffiSubscriber` (the held handle) | falls back (default) | **forward** to a new vtable slot; `supports_process_in_place` = slot non-NULL — **the activation gate** |
| CFFI vtable (`nros_rmw_vtable_t`) | no slot | **add** one append-to-tail in-place slot (RFC-0035 + `abi_version` bump) |
| zenoh-pico C backend | n/a | **populate** the slot → call the Rust `ZenohSubscriber` leaf |
| zpico `ZenohSubscriber` (Rust leaf) | implemented (subscriber.rs:1043) | the leaf the slot invokes; gains size-class pools (D1) |
| xrce (C backend) | no slot | populate the slot over its shared static pool, or leave NULL (buffered) |
| cyclonedds (C backend) | no slot | leave NULL (buffered) first; native loan path a later follow-up |
| mock (Rust, test-only) | falls back | buffered fallback permanently |

Land the executor scaffold first (done, behind the fallback), then the CFFI vtable
slot + zenoh-pico C impl (the activation), then the size-class pools. Backends
whose slot stays NULL are unaffected.

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

## Resolved decisions

All seven open questions are resolved (2026-06; discussion folded in here):

1. **Pool sizing → size-class pools (D1).** Not a single `SLOT_SIZE`. Two classes
   (`small` / `large`) split by a build-time threshold; a subscription is routed
   by its bounded max-serialized-size (RFC-0033). Avoids large-slot waste for
   small messages while keeping the shared-pool RAM win. No `#QoS × #size` matrix.
2. **Rollout → executor scaffold, then CFFI activation, then pools.** Because the
   executor reaches every non-mock backend through `CffiSubscriber` (Routing
   reality), the order is: (i) executor in-place selection + dispatch [done, Wave
   0.2, dormant]; (ii) the **CFFI in-place vtable slot + `CffiSubscriber`
   forwarding + zenoh-pico C impl** — the activation gate, on the critical path;
   (iii) size-class pools (D1) underneath. The buffered dispatch is retained as
   the fallback for any backend whose vtable slot is NULL; no flag-day; mock keeps
   the fallback permanently.
3. **cffi ABI → one optional callback-taking slot.**
   `process_raw_in_place(handle, ctx, fn(ctx, ptr, len)) -> bool`, mirroring the
   Rust `FnOnce` trait so the borrow cannot escape (a borrow/return pair would
   leak/UB if the caller forgets to return). Append-to-tail (RFC-0035); NULL →
   buffered fallback. Documented constraint: the in-place callback must not
   re-enter the *same* subscriber's receive (the backend holds its slot guard
   across it).
4. **Back-pressure → drop-oldest, never block the consumer.** Overflow drops at
   the pool; the shared transport thread is never blocked (head-of-line
   blocking). RELIABLE is honored upstream (reliable channel / DDS history), not
   by stalling the consumer. The per-sub depth budget is the isolation; pools
   sized `≥ Σ(depths)` make drops purely the intended `KEEP_LAST(N)` recycle.
   Overcommit + a reserved `reliable` sub-pool is a documented opt-in for
   tight-RAM deployments (the only case QoS-partitioning earns its keep).
5. **Multi-tier (Phase 228) → no new problem.** 228.C pins a node (and its
   subscriptions) to one tier, so per-subscription consumption stays
   single-consumer even multi-tier; only the cross-subscription pool allocator is
   cross-tier contention, and it already requires the thread-safe (`critical_section`)
   guard. Depends on the 228 node-pinning invariant.
6. **Polled vs threaded (T3) → `process_raw_in_place` is dispatch-only.** The
   method dispatches at most one *staged* message, uniform across backends. Link
   advancement (read + linearize) stays in the existing per-backend spin/poll
   hook (zpico RX thread for threaded; `zp_read`/`select` at spin for polled). The
   method is not overloaded with pumping.
7. **Scatter-gather (T1) → backend linearizes; no chunked trait variant.** The
   trait stays `&[u8]`; backends with non-contiguous payloads linearize into the
   slot (rmw_zenoh_cpp does the same with `as_vector`). A segmented in-place
   variant is revisited only on a profiled fragmented hot path.

## Relationship to other work

- **issue #8** (`docs/issues/0008-two-copy-receive.md`) — this RFC is its
  design-of-record; issue #8 flips to point here.
- **issue #7 / RFC-0033** — copy #2 elimination for borrowed types uses D2's
  borrow window; the `is_plain` gate decides availability per type.
- **RFC-0035** (RMW vtable ABI) — a cffi in-place take slot is an append-to-tail
  ABI change governed by 0035's evolution rule.
- **RFC-0006** — C-ABI-is-canonical; the in-place path must be expressible across
  the vtable, not Rust-only.
- **Phase 231** (`docs/roadmap/phase-231-zero-copy-receive.md`) carries the work
  items + acceptance tests and names this RFC in its `Implements:` header (per the
  RFC → roadmap → code flow). This RFC flips to **Stable** when Phase 231 lands.

## Acceptance (for the implementing phase)

- Default subscription receive on zpico performs **one** data-plane copy
  (transport → pool slot), with owned-message CDR as the only remaining copy and
  borrowed-message dispatch as zero data-plane copy.
- Static receive RAM is `Σ_class (N × slot_size)` (shared within each size
  class), independent of subscriber count; the 64 KB-image config sizes only the
  `large` pool's few slots, not `MAX_SUBSCRIBERS × DEPTH × largest_slot`.
- Per-subscription `KEEP_LAST(N)` is honored (N-deep, drop-oldest) from the
  subscription's size-class pool; with pools sized `≥ Σ(depths)` there is no
  cross-subscriber starvation.
- Non-zpico backends keep working via buffered fallback; no regression.
- `just ci` green; the existing `lending` / safety-e2e in-place tests pass
  against the new default dispatch.
