# Phase 396 — Pushed readiness: the wake says WHO, and the executor stops scanning

**Status (2026-08-28). Not started.** Implements
[RFC-0084](../design/0084-readiness-is-pushed-not-scanned.md).

The executor is already woken precisely — a `k_sem` signalled from the backend's
arrival path, sub-millisecond, ISR-capable. It then discards that precision and
walks every registered entity to find out what happened. This phase closes the
gap between the wake and the search, and lands the two phase-110.A traits that
have described this seam since they were written and never been wired to
anything.

**Depends on** nothing. Independent of
[issue 0852](../issues/0852-zephyr-serial-rx-is-polled-and-overruns.md) — that
is about how bytes reach the transport, this is about what the executor does
once they have.

---

## 1. What is there today

```
spin_once(timeout)
  ├─ next_deadline_ms()                    cap the wait against backend keepalive
  ├─ wake_flag.swap(false)
  ├─ has_async_wake ? NodeWake::wait_ms()  k_sem, ISR-safe
  │                 : drive_io(timeout)    block in the transport
  ├─ SCAN  for each entry: (meta.has_data)(ptr)     <-- up to 64 indirect calls
  ├─ evaluate Trigger (One / AllOf / AnyOf / Custom)
  └─ DISPATCH  BucketedFifoSet / BucketedEdfSet, by priority
```

| | today |
| --- | --- |
| wake latency | one `k_sem_give`, sub-ms, ISR-safe |
| wake precision | **binary** — "something happened" |
| finding out what | **O(N) indirect calls**, every spin |
| N | `MAX_CALLBACK_SLOTS = 64` (`spin.rs:1000`) |

`nros_rmw_runtime_wake_cb` (`spin.rs:673`) takes one argument, `ctx`, which
identifies the **executor**. The zenoh shim mirrors it into a process global
(`shim/mod.rs:184`) precisely so the real arrival hook can fire it "at the real
arrival point" — that hook stands where the entity identity is known and the ABI
it must call through has nowhere to put it.

## 2. Waves

### W1 — the token reaches the executor

Add `nros_rmw_runtime_wake_cb_tok(void *ctx, uint32_t token)`. Keep the existing
symbol; it forwards with `NROS_RMW_WAKE_TOKEN_UNKNOWN`. Add an `AtomicU64`
`ready_bits` to `WakeCtx`; the callback does one `fetch_or`.

**Done when** a zenoh subscription arrival sets a known bit, provable from a
unit test on the host with a mock session, and every other backend still links.

### W2 — registration mints the token

The entity-scoped wake registration already exists in the C API
(`nros_subscription_set_wake_callback` and the service/client twins). Hand the
`DescIdx` through at that point — the executor already assigns it.

**Done when** each registered entity's arrivals set that entity's bit and no
other, under a test with three subscriptions on distinct topics.

### W3 — `spin_once` reads bits instead of scanning

```
bits = ready_bits.swap(0)
if bits == 0 or any session reported UNKNOWN:   scan entries[]      (unchanged)
else:                                            seed the ReadySet from bits
```

`FifoReadySet` is already a `u64` bitmap with a `trailing_zeros()` pop
(`ready_set/mod.rs`), and it is already the dispatch input. Only the producer
changes.

**Done when** a build with only wake-capable backends performs **zero**
`has_data` calls on a spin that dispatches one callback, shown by a counter in
the test build.

### W4 — land phase-110.A, or delete it

`Activator` and `Dispatcher` (`executor/activator.rs`, `executor/dispatcher.rs`)
were defined by phase 110.A with the note that 110.A.b would rewire `spin_once`
through them. 110.A.b never landed: both carry `#[allow(dead_code)]`,
`ActivatorCtx` is a `PhantomData` shell, and there is no reference to either
outside their own files and doc comments.

W3 needs exactly the seam they describe — decide what fires, separately from
firing it — with two implementations: `ScanActivator` (today's loop) and
`PushedActivator` (read the bitmap), selected per session by whether a token was
supplied. Land them as that, or delete them. An abstraction that documents an
architecture the code does not have is worse than either.

### W5 — the arrived-vs-present rule

A bit says data **arrived**; `has_data` says data is **present now**. They
disagree when a callback drains its queue while an arrival races it.

Rule: **the bit is cleared when the callback is dispatched, not when the queue
is drained**, and a spurious dispatch to an empty queue is allowed. Callbacks
already tolerate this — `ReadySet::insert` has always been idempotent, so one
ready bit has always stood for any number of queued messages.

**Done when** a test drives arrival and drain concurrently and no message is
left undispatched (a spurious extra dispatch is a pass, a lost message is not).

### W6 — measure

Report, on the CANHUBK344 action image: `has_data` calls per spin before and
after, and wake-to-callback latency from the existing `wake-latency-probe`.

**Done when** the numbers are in this document. A phase that changes a hot path
and does not measure it has not finished.

## 3. Explicitly out of scope

- **Widening past 64 slots.** `ready_bits` is a `u64` because
  `MAX_CALLBACK_SLOTS` is 64. The `BitSet<N>` widening anticipated in
  `ready_set/mod.rs` needs a lock-free multi-word set-bit and should settle the
  token type with it (RFC-0084 §9).
- **A wait-on-many platform primitive.** Zephyr's `k_poll` is the native answer
  and FreeRTOS/POSIX have theirs; that belongs in the platform ABI as its own
  change, per [RFC-0076](../design/0076-platform-abi-ask-do-not-assume.md).
  Compatible with this phase, not required by it.
- **`rmw_wait`.** Moves multiplexing into the rmw layer across four backends.
  The token is the small half of that idea and is what this phase buys.

## 4. Risks

| risk | handling |
| --- | --- |
| a backend that needs poking to progress | W3 keeps the scan whenever any session reports UNKNOWN; RFC-0084 §9.2 is the open question and W1 must answer it before W3 lands |
| arrived-vs-present | W5, with the dispatch-clears rule stated above |
| ABI churn for out-of-tree backends | old callback symbol kept and forwards; a backend that does nothing still works |
