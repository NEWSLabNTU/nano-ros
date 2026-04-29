# Phase 99 — Zero-copy raw pub/sub API (loan + borrow)

**Goal:** Add a unified raw pub/sub API across all RMW backends that supports
true zero-copy where the backend offers it, and falls back to a
single-memcpy arena path where it doesn't. User code stays unchanged
across backends; lending capability is selected at compile time.

**Status:** v1 mostly landed (99.A–99.G + 99.D' wire-through complete; 99.H minimal; 99.I + 99.J + 99.K open)

**Priority:** Medium

**Depends on:** Phase 90 (PX4 uORB RMW backend) v1 complete — specifically
90.6 working examples + 90.7 SITL integration test must land first so the
new API can be migrated against a known-working baseline.

**Design rationale:** [docs/design/zero-copy-raw-api.md](../design/zero-copy-raw-api.md)
(to be authored as part of 99.A).

---

## Overview

Today's `EmbeddedRawPublisher::publish_raw(&[u8])` and
`RawSubscription::try_recv_raw(&mut [u8])` (introduced in Phase 90) cost
one memcpy each: user's buffer → backend on publish; backend → user's
buffer on receive. Acceptable for v1, but suboptimal for high-rate
sensor traffic and impossible to remove with the current API shape.

This phase introduces a **loan / borrow** API on the **raw** publisher /
subscription side **only** (see D7 in `docs/design/zero-copy-raw-api.md`):

- **Publish:** `EmbeddedRawPublisher::try_loan(len)` returns a
  `PublishLoan` exposing `&mut [u8]`. User writes directly into backend
  memory (or arena fallback). `commit()` finalizes.
- **Receive:** `RawSubscription::try_borrow()` returns a `RecvView`
  exposing `&[u8]` into the backend's recv buffer. Drop releases.

There is **no** typed `Publisher<M>::loan()` / `Subscription<M>::borrow()`.
The lent slot is `len` *bytes*; CDR ser/de needs the writer to discover
length, incompatible with `try_loan(len)`'s up-front contract. Typed
users either keep using `publish(&M)` (which CDR-encodes internally) or
drop down to the raw side after manually encoding into a `&[u8]`. POD
backends (uORB on PX4) sit on the raw side from the start — user owns
the `#[repr(C)]` struct↔bytes cast.

Both directions provide three blocking flavours:

| | non-blocking | blocking + executor | async |
|---|---|---|---|
| publish | `try_loan(len)` | `loan_with_timeout(len, exec, t)` | `loan(len).await` |
| receive | `try_borrow()` | `borrow_with_timeout(exec, t)` | `borrow().await` |

Same pattern as today's `Subscription::recv` etc.

**Compile-time lending capability** — each `nros-rmw-*` backend declares
a `lending` Cargo feature iff its FFI exposes a slot-borrow primitive.
nros-node aggregates via `rmw-lending`. `EmbeddedRawPublisher::try_loan`
body is `cfg`-gated: native slot if `rmw-lending` enabled; arena
fallback otherwise. No runtime branch.

Backend lending support today:

| Backend                           | Supports `lending`? | API                          |
| --------------------------------- | ------------------- | ---------------------------- |
| Zenoh-pico (default)              | No                  | n/a                          |
| Zenoh-pico (`unstable-zenoh-api`) | Yes                 | `z_bytes_writer_init`        |
| XRCE-DDS                          | Yes                 | `uxr_prepare_output_stream`  |
| uORB                              | **No**              | `orb_publish` always memcpys |
| DDS (full, w/ SHM)                | Yes                 | loaned message API           |

uORB never supports lending → user opting `nros/rmw-lending` w/ uORB
active = compile error from `where RmwPublisher: SlotLending` bound
unsatisfiable. Caught at build, not runtime.

---

## Sequencing

```
┌─────────────────────────────────────────┐
│ 95 prereq (must land first):            │
│   Phase 90.6 — PX4 talker/listener       │
│   Phase 90.7 — SITL integration test    │
└────────────────┬────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────┐
│ 99.A  design doc                        │
│ 99.B  trait surface (SlotLending,       │
│       SlotBorrowing in nros-rmw)        │
│ 99.C  PublishLoan + RecvView types      │
│ 99.D  arena impl (no-lending path)      │
│ 99.E  uORB backend (arena-only impl)    │
│ 99.F  zenoh-pico lending impl (gated)   │
│ 99.G  XRCE-DDS lending impl (gated)     │
│ 99.H  Promise-driven async loan/borrow  │
│ 99.I  migrate PX4 examples to new API   │
│ 99.J  migrate zenoh/xrce examples (opt) │
│ 99.K  benchmark + docs                  │
└─────────────────────────────────────────┘
```

Phases 99.E (uORB) lands the **arena-only path end-to-end** as the proof
that the abstraction is sound. 99.F/G add native lending opt-in. 99.I
migrates PX4 talker/listener (from 90.6) to the new API as the first
real consumer.

---

## Work items

### v1 (99.A–99.I — required)

- [x] 99.A — Design doc (`docs/design/zero-copy-raw-api.md`)
- [x] 99.B — Trait surface (`SlotLending`, `SlotBorrowing` in `nros-rmw`)
- [x] 99.C — `PublishLoan` + `RecvView` types in `nros-node`
- [x] 99.D — Arena impl (no-lending path) for `EmbeddedRawPublisher` / `RawSubscription`
- [x] 99.D' — `try_loan` / `try_borrow` wire-through to `<P as SlotLending/SlotBorrowing>` under `rmw-lending`
- [x] 99.E — uORB backend wiring (arena-only; `tests/loan_borrow.rs` 3/3 passes via std mock; enabling `rmw-lending` w/ uORB now fails to compile, satisfying the acceptance gate)
- [x] 99.F — Zenoh-pico lending impl behind `lending` feature (publisher SlotLending + subscriber SlotBorrowing)
- [x] 99.G — XRCE-DDS lending impl behind `lending` feature (publisher SlotLending + subscriber SlotBorrowing)
- [~] 99.H — Promise-driven `loan()` / `borrow()` futures (minimal v1: `borrow().await` and `loan().await` use `poll_fn` with self-wake yield; cancellation-safe pin-project-lite variant deferred to a follow-up)
- [ ] 99.I — Migrate PX4 talker/listener examples to loan/borrow (deferred; SITL gate stays under 90.7)

### Post-v1 (99.J + 99.K)

- [ ] 99.J — Migrate zenoh / xrce examples to loan/borrow (showcase
      cross-backend uniformity)
- [ ] 99.K — Benchmark vs `publish_raw`/`try_recv_raw` baseline; docs

---

### 99.A — Design doc

Author `docs/design/zero-copy-raw-api.md` capturing:

- Loan/borrow rationale (vs callback closures, vs plain `&[u8]` API)
- Promise-based unified async/sync/blocking
- Compile-time backend capability via Cargo `lending` feature
- `PublishLoan` enum: `Native(BackendSlot<'a>)` vs `Arena(ArenaSlot<'a>)`,
  collapsed to one variant per build
- `#[must_use]` + auto-discard-on-Drop semantics; explicit `discard()`
- `RecvView` `!Send + !Sync` lifetime tied to `&mut self`
- Per-backend support matrix
- Migration plan (Phase 99.I) and expected perf delta

**Files:** `docs/design/zero-copy-raw-api.md` (new)

### 99.B — Trait surface

Add to `packages/core/nros-rmw/src/traits.rs`:

```rust
/// Backend can lend a slot directly into its outbound buffer.
/// Backends with no native lending API (uORB) do not impl this trait.
#[cfg(feature = "lending")]
pub trait SlotLending: Publisher {
    type Slot<'a>: AsMut<[u8]> where Self: 'a;
    fn lend_slot(&self, len: usize) -> Result<Self::Slot<'_>, TransportError>;
    fn commit_slot(&self, slot: Self::Slot<'_>) -> Result<(), TransportError>;
}

/// Backend can lend a view directly into its received-message buffer.
#[cfg(feature = "lending")]
pub trait SlotBorrowing: Subscriber {
    type View<'a>: AsRef<[u8]> where Self: 'a;
    fn try_borrow(&mut self) -> Result<Option<Self::View<'_>>, TransportError>;
}

/// Cargo `lending` feature on `nros-rmw` is the marker. Each backend
/// crate (`nros-rmw-zenoh`, `nros-rmw-xrce`) decides whether to opt in
/// to the marker by enabling the upstream feature in its own
/// `[features.lending]`.
```

Backend Cargo.toml additions:

- `nros-rmw-zenoh`: `lending = ["unstable-zenoh-api", "nros-rmw/lending"]`
- `nros-rmw-xrce`: `lending = ["nros-rmw/lending"]`
- `nros-rmw-uorb`: **no `lending` feature** ever (uORB doesn't support).
- `nros-rmw-dds`: `lending = ["nros-rmw/lending"]` (when SHM transport added).

**Files:**

- `packages/core/nros-rmw/Cargo.toml` (`lending` feature)
- `packages/core/nros-rmw/src/traits.rs` (`SlotLending`, `SlotBorrowing`)
- `packages/zpico/nros-rmw-zenoh/Cargo.toml` (forward `lending`)
- `packages/xrce/nros-rmw-xrce/Cargo.toml` (forward `lending`)

### 99.C — `PublishLoan` + `RecvView` types

In `packages/core/nros-node/src/executor/handles.rs`:

```rust
#[must_use = "PublishLoan must be committed or discarded; dropping silently rolls back"]
pub struct PublishLoan<'a> { /* enum w/ cfg-gated variants */ }

impl<'a> PublishLoan<'a> {
    pub fn as_mut(&mut self) -> &mut [u8];
    pub fn try_commit(self) -> Result<(), CommitError>;
    pub fn commit(self) -> Promise<Result<(), CommitError>>;
    pub fn discard(self);  // no-op explicit
}

impl<'a> Drop for PublishLoan<'a> {
    fn drop(&mut self) {
        if !self.committed { /* release slot */ }
    }
}

pub struct RecvView<'a> {
    bytes: &'a [u8],
    _marker: PhantomData<*const ()>,  // !Send + !Sync
}

impl<'a> Deref for RecvView<'a> { type Target = [u8]; /* ... */ }
```

`LoanError` / `CommitError` enums w/ `WouldBlock` / `TooLarge` /
`Backend(TransportError)` variants.

**Files:**

- `packages/core/nros-node/src/executor/handles.rs` (PublishLoan, RecvView)
- `packages/core/nros-node/src/executor/types.rs` (LoanError, CommitError)
- Re-export from `nros-node::lib`, `nros::lib`, `nros::prelude`.

### 99.D — Arena impl

Per-publisher inline arena, const-generic sized:

```rust
pub struct EmbeddedRawPublisher<const TX_BUF: usize = DEFAULT_TX_BUF, const SLOTS: usize = 1> {
    handle: session::RmwPublisher,
    arena: TxArena<TX_BUF, SLOTS>,  // SLOTS slots of TX_BUF bytes each
}
```

`TxArena` = simple slot bitmap + `[[u8; TX_BUF]; SLOTS]`. `try_reserve` returns
`&mut [u8; TX_BUF]` + slot index, or `LoanError::WouldBlock` if all slots in use.
`release(slot_id)` clears bitmap on commit/discard.

`SLOTS = 1` default = single in-flight publish (simplest semantics, lowest
RAM). Users opting into pipelining: `EmbeddedRawPublisher<TX_BUF, 4>`.

**Files:**

- `packages/core/nros-node/src/executor/tx_arena.rs` (new)
- `packages/core/nros-node/src/executor/handles.rs` (wire arena into
  `try_loan` body when `not(feature = "rmw-lending")`)

### 99.E — uORB backend (arena-only impl, parity oracle)

Validate the abstraction end-to-end on the backend that **cannot** lend.
uORB's `commit_slot` simply calls existing `publish_raw` (which itself
calls `orb_publish` via the trampoline registry from Phase 90.2).

No backend code change required — arena-only path goes through
existing `Publisher::publish_raw`. nros-rmw-uorb does not impl
`SlotLending`. `EmbeddedRawPublisher::try_loan` body uses arena variant
unconditionally on uORB-active builds.

Add integration test: `tests/loan_borrow_uorb.rs` exercises full
loan/commit + try_borrow round trip via std mock broker. Verify same
bytes round-trip as Phase 90's `typeless_api.rs` test.

**Files:**

- `packages/px4/nros-rmw-uorb/tests/loan_borrow_uorb.rs` (new)

### 99.F — Zenoh-pico lending impl

Wrap `z_bytes_writer_init` (or equivalent stable API once available) as
`SlotLending` impl on `ZenohPublisher`. Gate behind
`nros-rmw-zenoh/lending` feature → `unstable-zenoh-api`.

`SlotBorrowing` impl uses `z_bytes_get_contiguous_view` (already gated
by `unstable-zenoh-api`).

Test: `tests/loan_borrow_zenoh.rs` w/ `lending` feature; verify true
zero-copy via byte-pointer comparison (slot pointer matches backend's
internal buffer).

**Files:**

- `packages/zpico/nros-rmw-zenoh/src/shim/publisher.rs` (SlotLending impl)
- `packages/zpico/nros-rmw-zenoh/src/shim/subscriber.rs` (SlotBorrowing impl)
- `packages/zpico/nros-rmw-zenoh/tests/loan_borrow_zenoh.rs`

### 99.G — XRCE-DDS lending impl

Wrap `uxr_prepare_output_stream` as `SlotLending` impl on `XrcePublisher`.
Gate behind `nros-rmw-xrce/lending`.

**Files:**

- `packages/xrce/nros-rmw-xrce/src/publisher.rs`
- `packages/xrce/nros-rmw-xrce/src/subscriber.rs`
- `packages/xrce/nros-rmw-xrce/tests/loan_borrow_xrce.rs`

### 99.H — Promise-driven async loan/borrow

`PublishLoan::commit() -> Promise<Result<(), CommitError>>` and
`EmbeddedRawPublisher::loan() -> Promise<Result<PublishLoan, LoanError>>`
implemented with `pin-project-lite` for cancellation safety. Pending
loan futures must release wait-queue reservations on Drop without
materializing a loan.

Pattern: arena's free-slot signal → wakes pending loan future. Same
pattern as `Subscription::recv()` but for the publisher side.

**Files:**

- `packages/core/nros-node/src/executor/loan_promise.rs` (new)
- `packages/core/nros-node/src/executor/borrow_promise.rs` (new)
- `Cargo.toml`: add `pin-project-lite` workspace dep

### 99.I — Migrate PX4 examples to loan/borrow

Rewrite `examples/px4/rust/uorb/talker` + `examples/px4/rust/uorb/listener`
(landed in 90.6) to use the new loan/borrow API. Drop the
`as_bytes(&msg)` cast helper — user writes directly into the loan's
`as_mut()` slice.

Verify the SITL integration test (90.7) still passes. This is the
**acceptance gate** for the v1 API: real PX4 module migrates cleanly.

**Files:**

- `examples/px4/rust/uorb/talker/src/main.rs` (rewrite)
- `examples/px4/rust/uorb/listener/src/main.rs` (rewrite)
- `packages/testing/nros-tests/tests/px4_e2e.rs` (update assertions)

### 99.J — Migrate zenoh / xrce examples (post-v1)

Convert `examples/native/rust/zenoh/{talker,listener}` to loan/borrow,
demonstrating cross-backend uniformity. With `nros/rmw-lending`
enabled, the same code achieves true zero-copy via `unstable-zenoh-api`.

### 99.K — Benchmark + docs (post-v1)

- Bench: `cargo bench` harness comparing `publish_raw` / `try_recv_raw`
  vs `loan` / `try_borrow` on each backend. Expected:
  - uORB: ~1 memcpy saved on publish (user-side); receive identical.
  - Zenoh-pico (lending): ~2 memcpys saved both directions.
  - XRCE-DDS (lending): ~2 memcpys saved both directions.
- Doc: `book/src/user-guide/zero-copy-raw-api.md`. Decision matrix:
  when to use loan/borrow vs `publish_raw` (latter still supported as
  convenience).

---

## Acceptance criteria

- [ ] `nros-rmw` defines `SlotLending` + `SlotBorrowing` traits gated by
      `lending` feature.
- [ ] `EmbeddedRawPublisher::try_loan` / `loan` / `RawSubscription::try_borrow`
      / `borrow` available on all backends.
- [ ] Compile error if user enables `nros/rmw-lending` w/ active backend
      that doesn't impl `SlotLending` (uORB).
- [ ] `PublishLoan` is `#[must_use]`; Drop = discard; explicit
      `commit()` / `discard()` semantics tested.
- [ ] `RecvView` is `!Send + !Sync`; lifetime tied to `&mut self`.
- [ ] Arena fallback works for backends without lending; per-publisher
      const-generic `TX_BUF` + `SLOTS`.
- [ ] uORB integration test (`loan_borrow_uorb.rs`) passes via std mock.
- [ ] Zenoh-pico lending test passes when `unstable-zenoh-api` enabled;
      byte-pointer assertion proves true zero-copy.
- [ ] XRCE-DDS lending test passes.
- [ ] PX4 talker/listener examples migrated; SITL test (90.7) still green.
- [ ] Promise-based futures cancel cleanly (drop releases wait-queue
      reservation); no leaks under cancel-storm test.
- [ ] `publish_raw` / `try_recv_raw` retained as convenience wrappers
      atop loan/borrow (no API removal).

---

## Notes

- **uORB's "save 1 memcpy"** is small in absolute terms (~tens of bytes
  for typical PX4 messages, ~hundreds of nanoseconds on Cortex-M).
  Real win is **API uniformity** — same code shape works on uORB +
  zenoh + xrce, gets zero-copy automatically when feature enables.
- **Per-publisher arena const-generic defaults:** `TX_BUF = DEFAULT_TX_BUF`
  (1024 bytes), `SLOTS = 1`. Power users override.
- **Async loan future cancellation** is the trickiest correctness
  property. Test under cancellation storm (spawn N tasks each
  dropping their loan future before resolution); arena slot count
  must return to all-free.
- **Loan size negotiation:** if user requests `len > TX_BUF`, immediate
  `Err(LoanError::TooLarge)`. No allocation, no fallback.
- **Mixed-mode (force arena despite backend support):** const-generic
  flag `FORCE_ARENA: bool = false` on `EmbeddedRawPublisher` lets users
  opt out of native lending for predictable cost. Edge case; can defer
  to post-v1.
- **MultiThreadedExecutor (Phase 94.H) interaction:** `RecvView` is
  `!Send + !Sync` so cannot cross threads — single-threaded constraint
  preserved. Arena slots are per-publisher, so multi-thread access to
  the same publisher requires the publisher to be `Sync`; default
  arena uses spin-lock or single-task assumption (decision in 99.D).

## Risks

- **Backend lending APIs may change.** Zenoh-pico's lending API is
  unstable; XRCE-DDS's `uxr_prepare_output_stream` is older but
  underused. If upstream removes them, our `lending` feature breaks.
  Mitigation: arena fallback always available; lending is opt-in.
- **Compile error UX** for `nros/rmw-lending` + uORB combination must
  produce a clear message (not a deep trait-bound error). Use
  `compile_error!` in the `nros-rmw-uorb` `lending` feature stub if
  the user accidentally enables it.
- **Promise + arena interplay:** if the executor halts while loan
  futures are pending, those futures hang forever. Same as existing
  `Subscription::recv` future; mitigation = `Promise::cancel()`.

## Prerequisites checklist (verify before starting)

- [ ] Phase 90.6 — PX4 talker/listener example landed
- [ ] Phase 90.7 — SITL integration test landed
- [ ] Phase 90's `typeless_api.rs` + `round_trip.rs` + `typed_pubsub.rs`
      all green
- [ ] `nros_node::Promise` API documented + extended w/
      `wait_with(executor)` if not already
