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

**Status:** In progress (2026-06-10). Wave 0 (trait surface + executor scaffold)
landed — dormant pending Wave 1 (CFFI in-place activation, the gate). See the
Routing-reality note under Work items.

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

### Wave 1 — CFFI in-place activation (the gate)  ← critical path

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

### Wave 2 — zpico size-class pools  (the RAM win)

- **2.1 — Pool data structure.** Replace `SUBSCRIBER_BUFFERS:
  [SubscriberBuffer; MAX_SUBSCRIBERS]` (per-sub fixed ring) with two shared pools
  (`small` / `large`), each `N × slot_size` slots + parallel attachment slots;
  per-slot `{ len, owner, timestamp }`. Build-time knobs:
  `ZPICO_POOL_{SMALL,LARGE}_{COUNT,SLOT_SIZE}` + threshold (extend
  `nros-zpico-build`). *Verify:* ghost-type tests updated; static-size assertions.
- **2.2 — Per-sub depth budget + drop-oldest.** A subscription draws from its
  size-class pool up to its `KEEP_LAST(N)`; overflow recycles its own oldest slot
  by timestamp (the micro-XRCE model). The C producer claims a free slot; the
  consumer releases after dispatch. Slot allocator guarded by `critical_section`
  on multi-threaded platforms (SPSC cursor-only on single producer). *Verify:* a
  `KEEP_LAST(1)` flooding sub holds exactly 1 slot and never evicts another sub's
  slots (pool not overcommitted).
- **2.3 — Size-class routing at registration.** Route each subscription to
  `small`/`large` by its bounded max-serialized-size (RFC-0033 capacity). Bake a
  warning when `Σ(member depths) > pool count` (overcommit). *Verify:* a mixed
  workspace (64 KB image sub + 64 B control sub) places each in the right pool;
  image sub does not consume a `large` slot from the control sub's budget.
- **2.4 — Delete the arena `BufferStrategy` for in-place subs.** Remove the
  trailing arena buffer from `SubBufferedEntry` / `SubBufferedRawEntry` on the
  in-place path; the entry holds handle + callback only. Keep `BufferStrategy` +
  `drain_into_buffer` alive solely as the fallback for non-in-place backends.
  *Verify:* arena per-entry size shrinks (static-RAM assertion); single-tier byte
  parity for non-subscription entries unchanged.

### Wave 3 — Acceptance + RAM proof

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

### Wave 4 — Other backends (incremental, post-MVP)

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
