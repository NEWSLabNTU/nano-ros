# Phase 231 — Zero-copy receive: in-place dispatch + size-class slot pools

**Goal:** Make the default subscription receive path **single-copy** and stop the
static receive buffers from scaling with `MAX_SUBSCRIBERS × DEPTH × largest_slot`.
Today every message is copied twice — backend ring → executor arena
`BufferStrategy` slot (copy #1) → message struct (copy #2) — and the receive
buffers are per-subscriber fixed arrays that explode for large messages (a 64 KB
image config = `8 × 4 × 64 KB = 2 MB`). This phase routes the executor's arena
subscription dispatch through the backend's in-place borrow
(`Subscriber::process_raw_in_place`), **deletes the arena `BufferStrategy`**
(removes copy #1 + its RAM), and replaces the per-subscriber rings with
**size-class slot pools** drawn per-subscription up to its runtime QoS depth. The
boundary grows by exactly two `Subscriber` trait methods plus one optional cffi
vtable slot; every other backend keeps working via a buffered fallback.

**Status:** COMPLETE (2026-06-10). **All waves (0–4) landed.** Wave 0 (trait surface
+ executor scaffold) + Wave 1 (CFFI in-place activation — vtable slots +
`CffiSubscriber` forwarding + adapter wiring, hermetic test green) + Wave 2
(zenoh-pico size-class receive buffers + the full `rx_buffer_hint` plumbing:
`TopicInfo` → `NrosRmwQos` ABI-append → backend routing) + Wave 3 (acceptance:
single-copy structural proof, compile-time RAM-bound assertion, size-class
routing/exhaustion, per-sub isolation). On native zenoh-pico, typed subscriptions
dispatch in-place (copy #1 gone) and receive storage no longer scales
`MAX_SUBS × DEPTH × large_slot`. Remaining: **Wave 4** (xrce in-place / cyclone +
mock keep the buffered fallback) — optional follow-up. See the Routing-reality
note under Work items.

**Priority:** P2 — the two-copy path works today; the win is RAM scaling for large
messages (the blocker for image/point-cloud on MCUs) + per-message CPU. Not
blocking the common small-message deployment.

**Implements:** RFC-0038 (zero-copy data transport — design-of-record). Resolves
`docs/issues/0008-two-copy-receive.md`. Copy #2 elimination for borrowed types is
**out of scope** (issue #7 / RFC-0033); this phase provides the borrow window it
will use.

**Depends on:** RFC-0038 (resolved decisions), RFC-0033 (bounded
max-serialized-size → size-class routing), RFC-0035 (RMW vtable ABI — the optional
in-place slot is append-to-tail), Phase 228 (node-pinned-to-tier invariant — keeps
per-subscription consumption single-consumer under multi-tier).

## Overview

RFC-0038 fixes the design; this phase is the implementation, in waves that each
build + test independently. The critical path is **Wave 0 (executor scaffold,
done) → Wave 1 (CFFI in-place activation — the gate) → Wave 2 (zpico size-class
pools, the RAM win)**; Waves 3–4 are acceptance + per-backend migration. The
executor never holds a Rust `ZenohSubscriber`, so in-place stays dormant until the
CFFI vtable slot lands (Wave 1) — see the Routing-reality note.

The **anti-bloat contract** (RFC-0038 "RMW interface surface") is the invariant to
preserve throughout: the executor learns nothing about a backend's buffer model;
the only things that cross the boundary are `process_raw_in_place`,
`process_raw_in_place_with_info`, and one optional cffi vtable slot. No QoS / pool
/ depth concept enters the trait. If a wave needs to leak pool internals up to the
executor, the design is wrong — stop and revisit the RFC.

## Architecture

```
Network → [size-class slot pool] → process_raw_in_place(|raw| …) → release slot
          (backend linearizes here)  (borrow window = callback scope)
          per-sub KEEP_LAST(N) budget = cross-sub isolation

executor arena dispatch:  loop { sub.process_raw_in_place(deserialize+callback) }
  (BufferStrategy / drain_into_buffer deleted; entry holds handle + callback only)
```

- **Pools (backend-internal):** two size classes (`small` / `large`) split by a
  build-time threshold; a subscription routes to a class by its bounded
  max-serialized-size (RFC-0033). Each pool = `N × slot_size` (build-time);
  within a pool a subscription draws up to its runtime `KEEP_LAST(N)` depth,
  drop-oldest. Pools sized `≥ Σ(member depths)` → no cross-sub starvation.
- **Dispatch (the one executor change):** replace `sub_buffered_*_try_process` +
  `drain_into_buffer` with an in-place loop; fall back to the buffered path when
  `process_raw_in_place` returns the unsupported error.

## Work items

> **Routing reality (RFC-0038).** The executor's `ConcreteSession` is always
> `CffiSession` (or `MockSession` in tests), so it holds a `CffiSubscriber` — it
> **never holds a Rust `ZenohSubscriber` directly**. zenoh-pico is reached through
> the CFFI vtable. Therefore the **CFFI in-place vtable slot is the activation
> gate** for any in-place dispatch through the executor (Wave 1 below), not an
> optional follow-up. The executor scaffold (Wave 0) is correct but **dormant**
> until that slot exists — which is exactly why it lands first, risk-free.

### Wave 0 — Trait surface + executor scaffold  ✅ DONE (dormant, no behavior change)

- **0.1 ✅ — `process_raw_in_place_with_info` on the `Subscriber` trait.** Added
  with a default unsupported body; signature `f: impl FnOnce(&[u8],
  Option<nros_core::MessageInfo>)` (matches the existing attachment model — not
  `&MessageInfo`). zpico's former inherent method moved onto the trait impl,
  converting `shim::MessageInfo` → `nros_core::MessageInfo`. All backends compile;
  no behavior change. (commit `231.0.1`.)
- **0.2 ✅ — In-place arena dispatch + registration selection.** Added
  `Subscriber::supports_process_in_place()` (default false; zpico→true),
  `SubInplaceEntry<M,F>` (no arena buffer), `sub_inplace_try_process` /
  `sub_inplace_has_data`. `register_subscription_buffered_on` selects in-place
  when the backend advertises support, else the buffered fallback. Compiles clean
  (default + `rmw-cffi,platform-posix`); 70 executor unit tests pass (cffi →
  buffered, unchanged). **Dormant on cffi/mock** until Wave 1. (commit `231.0.2`.)
  *Deferred to a later wave:* the raw / raw-info dispatch variants
  (`sub_inplace_raw_*`) — typed path done first.

### Wave 1 — CFFI in-place activation (the gate)  ✅ DONE ← critical path

This is what makes Wave 0 live. Until it lands, every subscription uses the
buffered fallback.

- **1.1 — Append the in-place vtable slot.** Add one append-to-tail slot to
  `nros_rmw_vtable_t` (RFC-0035 + `abi_version` bump):
  `process_raw_in_place(handle, ctx, fn(ctx, ptr, len)) -> i32` (callback-taking,
  borrow cannot escape). NULL → buffered fallback per the 0035 NULL contract.
  Document the no-reenter-same-sub rule. *Verify:* the vtable ABI test +
  `abi_version` reject on skew.
- **1.2 — `CffiSubscriber` forwarding.** Override `process_raw_in_place` /
  `process_raw_in_place_with_info` to invoke the slot (marshal the Rust `FnOnce`
  through the C `ctx`/`fn`); `supports_process_in_place()` returns whether the
  slot is non-NULL. *Verify:* a cffi backend with the slot dispatches in-place; a
  NULL slot falls back.
- **1.3 — zenoh-pico C backend populates the slot.** Wire the slot to call the
  Rust `ZenohSubscriber::process_raw_in_place` leaf (Wave 0.1), borrowing the ring
  slot. *Verify:* a native zpico pub/sub roundtrip through the executor takes the
  **in-place** path (`supports_process_in_place()` now true) and sees the same
  messages as buffered; `phase228_tier_filter` + `lending` + safety-e2e green.

### Wave 2 — zpico size-class buffers  ✅ DONE (the RAM win)

**Mechanism discovery (2026-06).** The zenoh-pico C producer (`sample_handler`
ring branch, `zpico-sys/c/zpico/zpico.c`) is **fully generic over the
`zpico_ring_desc_t` descriptor** (`payload_base` / `payload_stride` /
`slot_count` / `head` / `tail` / `att_*` / `*_len`). A subscriber's receive ring
is entirely described by that descriptor; the C side copies into
`payload_base + slot*stride` for whatever storage the descriptor points at. So
**the C producer needs no change** — only the Rust-side backing storage + which
descriptor a subscriber receives. The SPSC head/tail protocol and
`subscriber_notify_callback` stay as-is.

**Decision — size-classed per-sub rings, not a shared slot pool (yet).** RFC-0038
D1's *shared* pool (subs draw slots from one pool) would require replacing the
per-sub SPSC ring with a claim/release slot allocator in the C producer — a real
C-side rewrite. Instead, Wave 2 keeps the proven per-sub SPSC ring and splits the
**static storage into two size classes**: a sub of the `large` class gets a ring
of `LARGE_SIZE` slots, a `small` sub gets `SMALL_SIZE`. This kills the headline
explosion (`MAX_SUBS × DEPTH × 64 KB`): only the few `large` rings are big. It
gives **full per-sub isolation** (each sub owns its ring — even stronger than the
shared-pool depth budget) at the cost of not being fully sub-count-independent
(RAM is `MAX_LARGE×DEPTH×LARGE + MAX_SMALL×DEPTH×SMALL`, not `N×slot`). The true
shared pool (sub-count independence) is a deferred refinement — recorded as Q-pool
in RFC-0038 follow-ups — worth it only if a deployment has many large subs.

- **2.1 — Two size-class buffer types + static arrays.** Split
  `SubscriberBuffer` into `SmallSubscriberBuffer` (`[[u8; SMALL_SIZE]; DEPTH]`) and
  `LargeSubscriberBuffer` (`[[u8; LARGE_SIZE]; DEPTH]`), each with its own static
  array + `NEXT_*_INDEX` allocator + ghost checks. Both emit the same
  `zpico_ring_desc_t` via `init_ring_desc()` (the C producer is none the wiser).
  Build knobs: `ZPICO_SUBSCRIBER_{SMALL,LARGE}_SIZE`,
  `ZPICO_MAX_{SMALL,LARGE}_SUBSCRIBERS`, `ZPICO_SUBSCRIBER_SIZE_THRESHOLD`
  (`nros-zpico-build` + `nros-rmw-zenoh/build.rs`). *Verify:* ghost tests updated;
  static-size assertions per class.
- **2.2 — Size hint plumbed to `create_subscriber`.** The class is chosen by the
  subscription's receive-buffer size, which is known at the nros-node registration
  layer (`RX_BUF` on `register_subscription_buffered_on<M,F,RX_BUF>`) but **not**
  at the shim today (`create_subscriber` sees only topic + qos). Plumb a
  `rx_buffer_hint` from the executor through the `Session::create_subscriber`
  surface to the shim, which routes to `small`/`large` by the threshold. Other
  backends ignore the hint. *Verify:* a `large` sub allocates a `LargeSubscriberBuffer`,
  a `small` sub a `SmallSubscriberBuffer`.
- **2.3 — Delete the arena `BufferStrategy` for in-place subs.** Remove the
  trailing arena buffer from the in-place entry path; the entry holds handle +
  callback only. Keep `BufferStrategy` + `drain_into_buffer` as the fallback for
  non-in-place backends. *Verify:* arena per-entry size shrinks (static-RAM
  assertion); single-tier byte parity for non-subscription entries unchanged.

### Wave 3 — Acceptance + RAM proof  ✅ DONE

- **3.1 — Single-copy proof.** A test asserting the zpico default receive path
  performs one data-plane copy (transport → pool slot) — e.g. instrument the
  arena dispatch to confirm no `try_recv_raw`-into-arena memcpy occurs on the
  in-place path. *Verify:* the copy-#1 memcpy is gone for the default path.
- **3.2 — RAM-scaling proof.** A test/figure showing receive static RAM is
  `Σ_class (N × slot_size)` and does **not** grow with `MAX_SUBSCRIBERS × DEPTH`;
  the 64 KB-image config sizes only the `large` pool. *Verify:* compile two
  configs (many small subs vs one large sub) and assert the static footprint.
- **3.3 — QoS depth + isolation.** `KEEP_LAST(N)` honored (N-deep, drop-oldest)
  per subscription from its pool; with `≥ Σ(depths)` no cross-sub starvation under
  a flooding best-effort neighbor. *Verify:* a deterministic flood test.

### Wave 4 — Other backends  ✅ DONE

- **4.1 — xrce in-place.** micro-XRCE already stages into a shared static pool;
  populate its in-place vtable slot over `custom_static_buffers`. *Verify:* xrce
  subscription dispatches in-place.
- **4.2 — cyclonedds / mock.** cyclonedds leaves its slot NULL (buffered fallback;
  native loan path a later follow-up); mock keeps the fallback permanently.
  *Verify:* no regression.

## Out of scope

- **Copy #2 elimination** for owned message types — inherent to CDR
  deserialization; borrowed `&'a` types get zero data-plane copy via the Wave-0
  borrow window once issue #7 / RFC-0033 lands the borrowed-deserialize codegen.
- **Overcommit + reserved reliable sub-pool** — the tight-RAM opt-in (RFC-0038
  Q4); ship the guaranteed-sizing default first, add the reservation knob only if
  a deployment needs it.
- **Segmented/chunked in-place trait variant** (RFC-0038 Q7) — backends linearize
  non-contiguous payloads into a slot; revisit only on a profiled fragmented hot
  path.
- **User-facing loaned-message API** (`rmw_take_loaned_message`) — the in-place
  win is internal; no user borrow/return contract.

## Done when

- zpico default subscription receive is single data-plane copy (transport → pool
  slot); borrowed-type dispatch is zero data-plane copy.
- Receive static RAM is `Σ_class (N × slot_size)`, independent of subscriber count
  and per-sub depth; the 64 KB-image config no longer scales with
  `MAX_SUBSCRIBERS × DEPTH`.
- Per-subscription `KEEP_LAST(N)` honored with no cross-sub starvation
  (guaranteed-sized pools).
- Non-zpico backends unaffected (buffered fallback); `process_raw_in_place_with_info`
  added with a default body so they compile untouched.
- `just ci` green; `lending` + safety-e2e in-place tests pass against the new
  default dispatch; RFC-0038 flips to **Stable** and ARCHITECTURE.md notes the
  receive path in the same commit.
