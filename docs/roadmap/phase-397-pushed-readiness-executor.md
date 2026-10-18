# Phase 397 — Pushed readiness: the wake says WHO, and the executor stops scanning

**Status (2026-10-17). Not started; REVISED.** Implements
[RFC-0084](../design/0084-readiness-is-pushed-not-scanned.md).

**Revision 2026-10-17, read against `main`.**
- **Still true:** the scan, the one-argument wake callback, the `u64`
  `FifoReadySet` and the dead phase-110.A traits are all as §1 describes. The
  phase also gained a second payoff: the token's registration names the OWNING
  executor, which ends the wake fan-out's spurious wakes (phase-436 step 5).
- **What changed is W1/W2's premise.** The old W2 said entity-scoped wake
  registration "already exists in the C API", so the executor only had to
  pass its `DescIdx` through. The functions exist, renamed
  `rclc_polling_*_set_wake_callback`, but they end in `register_waker`. No
  `nros-rmw-cffi` handle implements that, and `rmw_vtable.h` has no
  per-entity slot to forward it to. So in every `rmw-cffi` build they are
  no-ops:
  [issue 1940](../issues/archived/1940-entity-wake-callback-is-inert-through-cffi.md).
  The token therefore needs a NEW vtable slot, which is now W1. Fixing 1940
  is that slot's first consumer, and it is what this phase stands on.
- **The backend set is two, not one.** Cyclone's `on_data_available(reader,
  arg)` receives the reader, so it can name the entity as zenoh's arrival
  hook can. XRCE and bare-metal smoltcp stay poll-only and report nothing.
- **W6's target image no longer exists** in tree (no CANHUBK344 action
  example), so W6 names images that do.

The executor is already woken precisely — a `k_sem` signalled from the backend's
arrival path, sub-millisecond, ISR-capable. It then discards that precision and
walks every registered entity to find out what happened. This phase closes the
gap between the wake and the search, and lands the two phase-110.A traits that
have described this seam since they were written and never been wired to
anything.

**Depends on** nothing. Independent of
[issue 0852](../issues/archived/0852-zephyr-serial-rx-is-polled-and-overruns.md) — that
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
| N | `MAX_CALLBACK_SLOTS = 64` (`executor/spin.rs`) |

`nros_rmw_runtime_wake_cb` (`executor/spin.rs`) takes one argument, `ctx`,
which identifies the **executor**. The backends' arrival hooks stand where the
entity is known: zenoh's `subscriber_notify_callback` and its service twins,
and Cyclone's `on_data_available(reader, …)`. They fire the session's wake
through the vtable's one session-scoped slot, `set_wake_callback`, so the
entity is dropped at the ABI. (zenoh's process-global mirror of that callback
became a per-session table with issue 1929, on 2026-10-16. That fixed which
EXECUTOR is woken, not which ENTITY.)

Wake sources the executor has today, and who can name an entity:

| backend | async wake | can name the entity |
| --- | --- | --- |
| zenoh | read task, `set_wake_callback` | yes: the arrival hook holds its subscriber, queryable or reply buffer |
| Cyclone DDS | data-available listener | yes: the listener receives the reader |
| XRCE | none (poll-only; RFC-0106 D5) | no; drive_io is the wait |
| uORB, bare-metal smoltcp | none | no |

## 2. Waves

### W1 — a per-entity wake slot in the RMW vtable (and issue 1940)

Today there is nowhere for an entity's wake to go: `rmw_vtable.h` carries
`set_wake_callback` per SESSION only, and `CffiSubscription` / `CffiService` /
`CffiClient` inherit `register_waker`'s no-op default. Add one slot per entity
kind, of the session slot's shape plus a token:

```c
/* NULL = this backend cannot attribute an arrival to the entity. */
rmw_ret_t (*subscription_set_wake)(rmw_subscription_t *sub,
                                   void (*cb)(void *ctx, uint32_t token),
                                   void *ctx, uint32_t token);
/* + service_set_wake, client_set_wake */
```

Then:
- the cffi handles forward `register_waker` through it, with
  `NROS_RMW_WAKE_TOKEN_UNKNOWN`, so issue 1940's C/C++ calls and
  `recv().await` reach a backend;
- a NULL slot makes those calls REPORT "unsupported", not `OK`;
- zenoh implements it in its arrival hooks, and Cyclone in
  `on_data_available`, from the reader.

The header is the SSoT (RFC-0054): regenerate with
`scripts/gen-abi-bindings.sh` and extend `check-rmw-abi-shape`'s map in the
same change.

**Done when** issue 1940's acceptance holds: a C test's
`rclc_polling_subscription_set_wake_callback` fires on a zenoh arrival, and an
XRCE build reports the call unsupported. Every other backend still links.

**Status (2026-10-18): landed except the Cyclone half.** The three slots, the
cffi forwarding, the adapter (which covers zenoh) and the UNSUPPORTED answer
shipped with issue 1940's fix — see its Resolution. The slot's callback is
`rmw_entity_wake_callback_t` (`void (*)(void *ctx, uint32_t token)`), the shape
above under a typedef. Cyclone still leaves the slots NULL and answers
unsupported; filling them from `on_data_available` is what remains of W1.

### W2 — the executor registers each entity with its token AND its owner

At registration the executor calls the new slot with its OWN wake context and
the entity's `DescIdx` as the token. `nros_rmw_runtime_wake_cb_tok(ctx,
token)` sets `ready_bits |= 1 << token` (one `fetch_or` on an `AtomicU64` in
`WakeCtx`), sets the flag and signals the primitive. The session-scoped
callback stays, forwarding `UNKNOWN`.

Because the context named is the owning executor's, an arrival wakes that
executor alone. The wake fan-out (RFC-0106 D6) stays only as the fallback for
a backend whose entity slot is NULL.

**Done when**:
- each registered entity's arrivals set that entity's bit and no other, under
  a test with three subscriptions on distinct topics;
- two executors over one session, a subscription on one: the other is not
  woken by its arrivals (phase-436 step 5's probe, inverted).

**Why the owner rides on the token** (phase-436 step 5,
2026-10-16). Several executors share one session through the wake fan-out
(RFC-0106 D6), and today every arrival wakes all of them. Measured on a native
host: 7-9 us of CPU per spurious wake, about 0.8 % of a core per idle member at
1 kHz. When the entity's registration names its executor's wake context, the
arrival wakes that executor alone, and the fan-out stays only as the fallback
for a backend that reports UNKNOWN. Fixing
[issue 1929](../issues/archived/1929-zenoh-async-wake-is-one-process-global-slot.md)
(zenoh's arrival wake was one process-global slot) was fixed ahead of this at
session grain (2026-10-16): a per-session wake table, so a second session's
executor no longer takes the first's arrivals. Entity grain is still this item.

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

Report, before and after:
- `has_data` calls per spin;
- wake-to-callback latency from the existing `wake-latency-probe`;
- CPU per idle executor at 1 kHz over a shared session (phase-436 step 5's
  probe: 6.8-9.4 us per spurious wake, about 0.8 % of a core, on a native
  host).

Run them on images that exist today: `workspace-rust-native` (host, zenoh),
`workspace-rust-native-cyclonedds` (host, Cyclone), and `workspace-rust-zephyr`
(Zephyr). The original target, a CANHUBK344 action image, is not in tree.

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
| ABI churn for out-of-tree backends | new slots are NULL-able and old symbols are kept; a backend that does nothing still works, and the entity calls now SAY it does nothing (issue 1940) |
| the slot lands and the executor half does not | W1 is useful alone: it fixes issue 1940. Ship it first, then W2-W3 |
