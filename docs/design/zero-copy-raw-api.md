# Zero-copy raw publish/subscribe API

**Owners:** core; **Status:** v1 (Phase 99.A–99.G + 99.D' wire-through landed; 99.H–99.K open).

## Goals

1. **No mandatory user-side copy** when publishing a typeless byte slice. Backends that natively lend (zenoh-pico, XRCE-DDS) hand the user a `&mut [u8]` pointing at the wire buffer. The user fills it in place. No memcpy on commit.
2. **No mandatory user-side copy on receive** when the backend can lend a read-only view of its received-message buffer.
3. **One source-level API**: identical user code on every backend. The compile-time selection between *native lending* and *per-publisher arena fallback* happens behind the scenes.
4. **Drop-safe**: an unused `PublishLoan` does not crash and does not silently publish stale bytes; the slot is returned to the free pool. Compile-time `#[must_use]` catches accidental drops in user code.
5. **`!Send + !Sync` views** so a `RecvView` cannot escape the receiving task or cross a `.await` (which would block subsequent receives on the same subscription).

## Non-goals

- Zero-copy across **all** backends without preconditions. uORB has no native lending; the arena fallback is the best we can do, with a single memcpy on commit. DDS over UDP cannot lend cross-process without shared memory; its native lending lands when an SHM transport is added.
- Replacing `publish_raw` / `try_recv_raw`. Both stay as light convenience wrappers (encode CDR locally, hand to backend in one shot). Loan/borrow is the path you take when you care about the copy count.
- Making a `PublishLoan` `Send`. Single-task ownership simplifies arena release on Drop and keeps the backend slot's `Drop` impl single-threaded.

## API summary

```rust
// publish side
let mut loan = pub.try_loan(len)?;     // Result<PublishLoan<'_, TX_BUF>, LoanError>
loan.as_mut().copy_from_slice(&bytes); // user-fill
loan.commit()?;                        // hand to backend (arena memcpy OR commit_slot)

// receive side
match sub.try_borrow()? {              // Result<Option<RecvView<'_>>, NodeError>
    Some(view) => process(&view),      // Deref<Target = [u8]>
    None => /* nothing pending */,
}
```

Three variants, all valid:

| Method | Returns | Blocks? |
|---|---|---|
| `try_loan(len)` / `try_borrow()` | now-or-never | no |
| `loan_with_timeout` / `borrow_with_timeout` | sync, spins executor | yes (bounded) |
| `loan().await` / `borrow().await` | async, future-based | yes (cooperative) |

(`loan().await` is Phase 99.H, future-based with cancellation safety.)

## Layering

```
                      Application (user code)
                              │
                              ▼
                       nros::prelude::*
                              │
                              ▼
       EmbeddedRawPublisher<TX_BUF>          RawSubscription<RX_BUF>
       ┌──────────────────────────┐          ┌─────────────────────┐
       │ try_loan      (99.D')    │          │ try_borrow (99.D')  │
       │ loan_with_timeout        │          │ borrow_with_timeout │
       │ loan().await    (99.H)   │          │ borrow().await      │
       └──────────────────────────┘          └─────────────────────┘
                       │  cfg-gated dispatch
        ┌──────────────┴──────────────┐
        │                             │
        ▼ feature = "rmw-lending"     ▼ default
  <RmwPublisher as SlotLending>   TxArena<TX_BUF> + publish_raw
  ::try_lend_slot() →             (single memcpy on commit)
  Slot<'a> ← writes wire buffer
  ::commit_slot(slot)
        │
        ▼
   nros-rmw-zenoh / nros-rmw-xrce
   (z_bytes_from_static_buf      / uxr_prepare_output_stream
   z_bytes_get_contiguous_view)  / per-slot view + locked flag)
```

## Design decisions

### D1 — Compile-time backend capability via Cargo features

`nros-rmw` ships the `SlotLending` + `SlotBorrowing` traits behind a `lending` feature. Each backend crate that *can* impl them forwards its own `lending` feature to `nros-rmw/lending`:

- `nros-rmw-zenoh/lending` → real native lending via zenoh-pico aliased publish + contiguous-view borrow.
- `nros-rmw-xrce/lending` → `uxr_prepare_output_stream` + slot view.
- `nros-rmw-uorb` — no `lending` feature ever (uORB has no equivalent of `prepare_output_stream`).
- `nros-rmw-dds` — no `lending` feature today; lands when SHM transport is added.

User opts via `nros/rmw-lending`. If the active backend doesn't expose `lending`, cargo's feature-unification fails at the crate level — clearer than a missing-trait error at link time.

This is not a runtime fallback. With `rmw-lending` off, `EmbeddedRawPublisher::try_loan` always uses the per-publisher arena. With `rmw-lending` on against a backend that supports it, `try_loan` always uses the native path. There's no "tried native, fell back to arena" branch — the type itself differs (`PublishLoan` carries a different field across the cfg).

### D2 — `PublishLoan` is `#[must_use]` + auto-discard on Drop

```rust
#[must_use = "PublishLoan must be committed or discarded; dropping silently rolls back"]
pub struct PublishLoan<'a, const TX_BUF: usize> { ... }
```

Drop releases the arena slot (no-lending build) or the backend Slot's own Drop releases the wire buffer (lending build). Bytes are not published. The `#[must_use]` lint catches every accidental drop except the trailing one in `_ = pub.try_loan(len)?` (which is a deliberate discard).

### D3 — `RecvView` is `!Send + !Sync` + lifetime tied to `&mut self`

```rust
pub struct RecvView<'a> {
    /* ... */
    _marker: PhantomData<*const ()>,  // !Send + !Sync
}
```

`!Send` prevents the view from crossing a `.await` (which would block the underlying subscriber's next receive). `!Sync` prevents shared-borrow patterns that would let two tasks race on the same backend slot. Lifetime `'_` is the lifetime of `&mut self` on `try_borrow` — the view dies before the next `try_borrow` / `try_recv_raw` call.

Drop releases the backend's "locked" flag (lending) or simply releases the `&self.buffer` borrow (arena). There is at most one `RecvView` alive per `RawSubscription` at a time; the borrow checker enforces this without runtime cost.

### D4 — Single-slot arena (`SLOTS = 1`) is the default

`TxArena<TX_BUF>` is single-slot. Concurrent `try_loan` calls on the same publisher return `LoanError::WouldBlock`. This keeps each publisher's RAM cost to `TX_BUF` bytes plus an `AtomicBool` busy flag.

A future opt-in for `SLOTS = N` could pipeline N in-flight publishes per publisher. Not in v1 — the use-cases that need pipelining (high-rate sensor streams) are the same use-cases that should switch to native lending anyway.

### D5 — `LoanError::WouldBlock` instead of returning `Option`

`try_loan` returns `Result<PublishLoan, LoanError>` (not `Result<Option<…>, …>`). `WouldBlock` is the "slot busy" sentinel; `TooLarge` is the static error; `Backend(TransportError)` is the catastrophic case. Explicit > ambiguous `Option`.

`try_borrow` returns `Result<Option<RecvView<'_>>, NodeError>` because `Option::None` here is the "no message yet" idiom — distinct from "subscriber unavailable" (which is `Err`).

### D6 — Promise-driven async (Phase 99.H)

`loan().await` and `borrow().await` are pin-projected futures. Cancelling the future before `.await` returns must:

- Release any slot reservation taken inside `poll`.
- Not materialise a `PublishLoan` that the caller never sees.

Implementation: the future holds an enum state (`Idle | Reserving | Ready(slot)`). `poll` advances the state; `Drop` runs cleanup against whatever state is reached. `pin-project-lite` is a workspace dep already (used elsewhere); no new transitive deps.

## Per-backend support matrix

| Backend | `lending` feature | Native publish | Native receive | v1 status |
|---|---|---|---|---|
| zenoh-pico (Phase 99.F) | yes | `z_bytes_from_static_buf` aliased publish (no payload copy; attachment still copied) | `z_bytes_get_contiguous_view` (`unstable-zenoh-api`) | landed |
| XRCE-DDS (Phase 99.G) | yes | `uxr_prepare_output_stream` direct write into `ucdrBuffer.iterator` | per-slot view + `locked` flag (subscriber buffers static, slotted) | landed |
| dust-dds (Phase 99) | no (today) | `publish_raw` only | `try_recv_raw` only | gated on SHM transport |
| uORB (Phase 90) | never | arena-only — `commit` does `orb_publish` (1 memcpy into the uORB topic struct) | `try_recv_raw` only — uORB callback writes into static slot | arena fallback always |
| cffi (`nros-rmw-cffi`) | no | `publish_raw` only | `try_recv_raw` only | not on the lending path |

## Migration plan

| Phase | Examples touched |
|---|---|
| **99.I (v1 gate)** | `examples/px4/rust/uorb/{talker,listener}` — first real-world consumer; passes the SITL E2E (90.7) unchanged. |
| 99.J (post-v1) | `examples/native/rust/zenoh/{talker,listener}` and the equivalent xrce examples — same source code, recompile with `nros/rmw-lending` against zenoh+`unstable-zenoh-api` to demonstrate the cross-backend uniformity. |
| 99.K (post-v1) | `cargo bench` harness measuring `publish_raw`/`try_recv_raw` vs `loan`/`try_borrow` per backend; user-guide chapter `book/src/user-guide/zero-copy-raw-api.md` with a decision matrix. |

Existing user code that calls `publish_raw` / `try_recv_raw` keeps working unchanged. The two APIs coexist.

## Expected perf delta

Estimates per published / received message, relative to `publish_raw` / `try_recv_raw` baseline. Actual numbers under 99.K bench.

| Backend | Publish save | Receive save |
|---|---|---|
| uORB | ~1 memcpy (the user-side encode buffer copy) | identical (uORB callback already writes in place) |
| zenoh-pico (lending) | ~2 memcpys (user→backend buffer, backend→wire buffer collapses to one) | ~1 memcpy (avoid `try_recv_raw`'s copy into `RawSubscription::buffer`) |
| XRCE-DDS (lending) | ~2 memcpys (same pattern) | ~1 memcpy |
| dust-dds (no lending) | identical | identical |

For 30 Hz × 1 KB messages on the autoware_sentinel control loop (the original motivator), the publish-side save is ~60 KB/s of memcpy avoided per topic — small in absolute terms, but eliminates a fixed-cost overhead that scales linearly with topic count. The bigger win is on high-rate sensor streams (camera, LiDAR) where the per-message size dwarfs everything else.

## References

- `nros-rmw/src/traits.rs:1024` — `SlotLending` trait
- `nros-rmw/src/traits.rs:1050` — `SlotBorrowing` trait
- `nros-node/src/executor/handles.rs::EmbeddedRawPublisher` / `RawSubscription` / `PublishLoan` / `RecvView`
- `nros-rmw-zenoh/src/shim/{publisher,subscriber}.rs` — Phase 99.F impls
- `nros-rmw-xrce/src/lib.rs` (XrceSlot, XrceView) — Phase 99.G impls
- Phase 99 roadmap: `docs/roadmap/phase-99-zero-copy-raw-api.md`
