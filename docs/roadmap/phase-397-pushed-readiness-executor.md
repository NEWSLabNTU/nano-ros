# Phase 397 — Pushed readiness: the wake says WHO, and the executor stops scanning

**Status (2026-10-19). W1 DONE; W2 DONE (zenoh and Cyclone); W3 onward not started.** Implements
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

### W1 — a per-entity wake slot in the RMW vtable (and issue 1940) — DONE 2026-10-18

Today there is nowhere for an entity's wake to go: `rmw_vtable.h` carries
`set_wake_callback` per SESSION only, and `CffiSubscription` / `CffiService` /
`CffiClient` inherit `register_waker`'s no-op default. Add one slot per entity
kind.

**Landed as upstream's own trio, not as the `*_set_wake(cb(ctx, token), ctx,
token)` this item first proposed:**

```c
/* NULL, or NROS_RMW_RET_UNSUPPORTED = cannot attribute an arrival. */
rmw_ret_t (*subscription_set_on_new_message_callback)(
    rmw_subscription_t *subscription, rmw_event_callback_t callback,
    const void *user_data);
/* + service_set_on_new_request_callback, client_set_on_new_response_callback */
```

A token argument is redundant once `user_data` is per entity, and the upstream
shape turned three `declined` parity rows (issue 0960) into exact matches
(`check rmw-abi-shape`, `check rmw-api-parity`). `number_of_events` is a lower
bound and pre-registration samples are not replayed; the header says both.
RFC-0084's 2026-10-18 amendment records the change.

What landed:
- `nros_rmw::{Subscription,ServiceTrait,ClientTrait}::register_waker` returns
  `bool`: whether a wake will come. Default `false`.
- the cffi handles keep the caller's waker in a per-entity slot and forward
  `register_waker` through the new slots (`nros-rmw-cffi/src/entity_wake.rs`);
- the Rust adapter fills the slots for every Rust backend, so zenoh serves them.
  Its waker re-arms itself, because zenoh's `AtomicWaker` gives a waker up on
  each wake and the ABI says a registration persists;
- the zenoh client keeps a waker registered before any request exists, and
  arms each new get with it;
- every C and C++ wake setter — the three entity ones and the seven action
  channels — returns `UNSUPPORTED` when the backend cannot wake, not `OK`.
- Cyclone, XRCE and uORB leave the slots NULL. **Cyclone's `on_data_available`
  half is NOT done**; it moves to W2, where the executor first needs it.

Acceptance, as run:
- `just check c` runs `entity_wake_callback.c` against the stub backend: three
  kinds, three arrivals each, delivered; with the slots answering UNSUPPORTED,
  each setter returns `RCL_RET_UNSUPPORTED`.
- `cffi_smoke::cffi_subscription_waker_fires_on_each_arrival` (in `test-all`)
  registers a waker once on a `CffiSession` subscription over a real zenoh
  router and sees it woken on at least three arrivals. With the adapter's
  re-arm removed it reads 1, so the test catches that mutation.
- A per-backend XRCE run reporting UNSUPPORTED was not done. It follows from
  XRCE's NULL slot through the same code path the stub's UNSUPPORTED arm drives.

### W2 — the executor registers each entity with its token AND its owner — DONE 2026-10-19

**Landed (W2.a).** Every subscription, service and client registration
(the 19 sites in `executor/spin.rs`) calls `Executor::arm_entity_wake` after
the entry is written into the arena. That hands the handle's `register_waker`
a waker from `executor/entity_wake.rs`. The waker carries no separate record:
it holds a strong reference to the owning executor's `WakeCtx` and keeps the
slot in the low bits of that pointer. `WakeCtx` is `repr(align(64))` to make
room for them.

Waking it calls `WakeCtx::mark_ready(slot)`: `ready |= 1 << slot` on a new
`AtomicUsize`, then the same flag, probe and signal as the session callback.
That shared step is now one function, `WakeCtx::signal`. The word is a
`usize` because a 32-bit target may have no 64-bit atomics, so there it
names 32 slots, and an entry above that keeps the session wake. The executor also
records which slots a backend accepted, in `entity_attributed`. W3 needs that
mask, because a clear bit is an entry only the session wake still covers.

Two choices differ from the text this item first had:

- **No `nros_rmw_runtime_wake_cb_tok`.** The executor reaches W1's slot through
  the handle's `register_waker`, so the per-entity record is a Rust `Waker`, not
  a C `user_data`. On the cffi route the adapter's `Hosted<T>` and
  `WakerSlot` (W1) carry it across the ABI. The waker owns an `Arc` reference,
  so a backend that keeps a clone past the entry's release cannot dangle.
- **The session wake is skipped by the BACKEND, not by the fan-out.** The
  fan-out cannot know which entity an arrival was for, so it cannot be the
  one to skip it. zenoh's arrival hooks (subscriber, queryable, reply) now go
  through `shim::wake_entity_or_session`, which wakes the entity's
  `AtomicWaker` when it holds one and fires the session wake only when it is
  empty.
  - Empty is still the case the session wake exists for: an entity nobody
    attributed, an async future whose wake was already taken, or
    `Promise::wait`, which registers nothing.
  - `take` + `wake` is exactly `AtomicWaker::wake`, so a one-shot async caller
    sees no change.

**Done-when, both measured** (`nros-tests/tests/entity_wake.rs`, in
`just check required-features-tests`):
- Three subscriptions on distinct topics: each arrival sets that
  subscription's bit and no other. There is also a negative control: no bits
  are set before the first publish.
- Two executors over one session (the tier shape, `open_with_session_handle`),
  with a subscription on the first. Under an arrival every 50 ms, the second
  sleeps out its whole 800 ms `spin_once`, and the control asserts the
  arrivals reached the owner.
- Both tests publish on the subscriber's own session. zenoh-pico's
  local-subscriber delivery runs the same arrival hook a remote sample does,
  and the POSIX pool holds one session per process.

Mutations, each red then restored green:
- Firing the session wake beside the entity wake fails the second test:
  `other` returned after 7.92 µs.
- Disabling `arm_entity_wake` fails both tests, with no slot attributed.

**Landed (W2.b) — Cyclone fills W1's slots.** Each slot puts a
data_available listener on the entity's own READER, carrying the registration
(`EntityWake`, `entity_wake_set` in `session.cpp`). Cyclone invokes a reader's
own listener instead of the participant's, where the session wake lives, so
an attributed arrival calls its entity's callback and the session wake does
not fire for it. That is the same split zenoh makes, decided by Cyclone's
listener lookup rather than by a waker cell.

Clearing removes the reader's listener, and its arrivals reach the session
wake again. `cb` and `user_data` change only while no listener is attached,
because `dds_set_listener` waits out a callback in progress. A
`NO_FOREIGN_WAKE` build reports `UNSUPPORTED`, for issue 1237's reason.

`tests/entity_wake.cpp` (Cyclone suite, 35/35) covers three cases:
- registered: the entity callback fires with its `user_data`, and the session
  wake does not;
- unregistered on the same session: the session wake still fires;
- cleared: the arrival goes back to the session wake.

Not attaching the listener fails the first case.

That test also found
[issue 1963](../issues/1963-cyclone-late-wake-callback-misses-existing-readers.md):
a session wake installed AFTER a reader exists never fires for that reader.
Every in-tree caller installs at session open, so it is latent.

**Why the owner rides on the token** (phase-436 step 5,
2026-10-16). Several executors share one session through the wake fan-out
(RFC-0106 D6), and before this item every arrival woke all of them. Measured
on a native host: 7-9 us of CPU per spurious wake, about 0.8 % of a core per
idle member at 1 kHz.
[Issue 1929](../issues/archived/1929-zenoh-async-wake-is-one-process-global-slot.md)
(zenoh's arrival wake was one process-global slot) was fixed ahead of this at
session grain on 2026-10-16: a per-session wake table, so a second session's
executor no longer takes the first's arrivals. This item is the entity grain.

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
| the slot lands and the executor half does not | W1 is useful alone: it fixed issue 1940, and landed first (2026-10-18) |
