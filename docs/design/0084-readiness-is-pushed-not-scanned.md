---
rfc: 0084
title: "Readiness is PUSHED by the arrival path, not scanned by the executor — the wake says who, not just that"
status: Draft
since: 2026-08
last-reviewed: 2026-10-18
implements-tracked-by: [phase-397]
amends: [rfc-0052]
supersedes: []
superseded-by: null
---

# RFC-0084 — Readiness is pushed, not scanned

> **Amendment 2026-10-17.** §4.2's premise does not hold on `main`: the
> entity-scoped wake functions end in `register_waker`, which no
> `nros-rmw-cffi` handle implements and no vtable slot carries, so they are
> no-ops ([issue 1940](../issues/archived/1940-entity-wake-callback-is-inert-through-cffi.md)).
> The token therefore needs a new per-entity vtable slot, specified in §4.2
> below. Two further changes: Cyclone can supply tokens as well as zenoh
> (§8), and the registration also names the OWNING executor, which retires
> RFC-0106 D6's spurious wakes (§4.5).

> **Amendment 2026-10-18.** The slot landed (issue 1940, phase-397 W1), and
> not in the shape §4.2 first gave it. It is upstream's own trio,
> `{subscription,service,client}_set_on_new_*_callback`, with upstream's
> `rmw_event_callback_t` — `(const void *user_data, size_t n)` — rather than a
> new `*_set_wake(cb(ctx, token), ctx, token)`. A token argument is redundant
> once `user_data` is per ENTITY: the registration already names one entity, so
> the executor points `user_data` at a record holding its wake context and the
> entity's `DescIdx`. Mirroring upstream turned three `declined` rows of the
> parity map (issue 0960) into exact matches. §4.2 is rewritten to match; §4.1's
> `nros_rmw_runtime_wake_cb_tok` survives as the RUNTIME's side of that
> callback, not as an ABI type.

> The executor is already woken *precisely* — a k_sem signalled from the
> backend's arrival path, sub-millisecond, ISR-capable. It then throws that
> precision away and searches every registered entity to find out what
> happened. This RFC closes the gap between the wake and the search.

## 1. The problem, counted

`Executor::spin_once` wakes, then does this
(`nros-node/src/executor/spin.rs`, activation phase):

```rust
for (i, meta) in self.entries.iter().enumerate() {
    ...
    if unsafe { (meta.has_data)(data_ptr) } { ... }
}
```

One indirect call per registered entity, **every spin, regardless of what woke
it**. `MAX_CALLBACK_SLOTS = 64` (`spin.rs:1000`), so the ceiling is 64 indirect
calls through function pointers the compiler cannot devirtualise, each one
typically reaching into a backend ring buffer, to discover the one or two bits
that changed.

The asymmetry is the point:

| | today |
| --- | --- |
| wake latency | one `k_sem_give`, sub-ms, ISR-safe |
| wake precision | **binary** — "something happened" |
| find out what | **O(N) indirect calls** |
| N | up to 64 |

The wake path already knows exactly which entity received data. The signal it
raises discards that, and the executor rediscovers it by exhaustive search.

## 2. Why the information is lost

`nros_rmw_runtime_wake_cb` (`spin.rs:673`) takes one argument:

```rust
pub(crate) unsafe extern "C" fn nros_rmw_runtime_wake_cb(ctx: *mut c_void) {
    let wake = unsafe { &*(ctx as *const WakeCtx) };
    wake.flag.store(true, SeqCst);
    if let Some(nw) = wake.node_wake.as_ref() { nw.signal(); }
}
```

`ctx` identifies the **executor**, not the entity. The zenoh shim mirrors the
callback into a process-global (`shim/mod.rs:184`, `set_runtime_wake_cb`) so
the real arrival hook — `subscriber_notify_callback` and its service twin —
can fire it "at the real arrival point". That hook is standing at the exact
place where the identity is known, and the ABI it must call through has nowhere
to put it.

So the executor is woken by the right event and then cannot act on it.

## 3. What already exists

Most of the destination is built. `ReadySet`
(`executor/ready_set/mod.rs`) is a presence **bitmap**:

```rust
pub(crate) struct FifoReadySet<const N: usize> { bits: u64 }

fn insert(&mut self, job: ActiveJob) -> Result<(), Overflow> {
    self.bits |= 1u64 << job.desc_idx; Ok(())
}
fn pop_next(&mut self) -> Option<ActiveJob> {
    let idx = self.bits.trailing_zeros() as DescIdx;
    self.bits &= !(1u64 << idx);
    ...
}
```

`insert` is idempotent by construction, `pop_next` is a `CLZ`-class single
instruction on Cortex-M7, and `BucketedFifoSet` / `BucketedEdfSet` already
layer priority buckets and EDF ordering on top. `MAX_CALLBACK_SLOTS = 64`
matches the `u64` width exactly.

**The set is the right shape and it is already the dispatch input.** What is
missing is only that nothing writes into it except the scan.

## 4. Proposal

### 4.1 The wake carries a token

Extend the runtime wake callback with an entity token:

```c
/* today */
void nros_rmw_runtime_wake_cb(void *ctx);

/* proposed */
void nros_rmw_runtime_wake_cb_tok(void *ctx, uint32_t token);
```

`token` is opaque to the backend: it is whatever the executor handed out at
registration time. The executor mints it as the `DescIdx` it already assigns,
so the callback body becomes:

```rust
wake.ready_bits.fetch_or(1u64 << token, Ordering::SeqCst);
wake.flag.store(true, Ordering::SeqCst);
if let Some(nw) = wake.node_wake.as_ref() { nw.signal(); }
```

A `fetch_or` on an `AtomicU64` — lock-free, ISR-callable, and on a 32-bit
target `portable_atomic` supplies it. `NROS_RMW_WAKE_TOKEN_UNKNOWN` (all ones)
means "I know something arrived but not what", and degrades to the scan for
that spin. That is the honest answer for a backend that cannot attribute an
arrival, and it keeps the token optional rather than mandatory.

### 4.2 Registration hands the token out

`set_wake_callback` is session-scoped, and it is the only wake slot in
`rmw_vtable.h`. The public entity-scoped functions
(`rclc_polling_{subscription,service,client}_set_wake_callback`, their C++
twins, and `recv().await`'s waker) end in `register_waker`, which reaches no
backend through cffi (issue 1940). So the vtable gains one NULL-able slot per
entity kind, upstream's own (landed 2026-10-18, phase-397 W1):

```c
typedef void (*rmw_event_callback_t)(const void *user_data, size_t number_of_events);

rmw_ret_t (*subscription_set_on_new_message_callback)(
    rmw_subscription_t *subscription, rmw_event_callback_t callback,
    const void *user_data);
/* + service_set_on_new_request_callback, client_set_on_new_response_callback */
```

There is no token argument: `user_data` is per entity, so it carries the
token. The executor registers each entity with `user_data` pointing at a
record of its own wake context and the entity's `DescIdx`, and the runtime's
callback (§4.1's `_tok` body) reads both from there. The cffi handles already
forward `register_waker` through the same slot, so issue 1940's callers reach
a backend. A NULL slot, or a slot answering UNSUPPORTED, means the backend
cannot attribute arrivals: the executor keeps the session callback for it, and
the user-facing calls report "unsupported" instead of `OK`.

Two behaviours differ from upstream, on purpose, and the header states both.
`number_of_events` is a lower bound (a backend may call once per arrival with
1), so no backend has to track per-entity pending depth — the RAM argument
issue 0960 declined the trio on, kept. And samples that arrived before the
registration are not replayed; a caller registers, then checks `has_data`.

### 4.3 The scan becomes the fallback

```
spin_once:
  wait on wake primitive
  bits = ready_bits.swap(0)
  if bits == 0 or any session reported UNKNOWN:
      scan entries[]   <-- unchanged code path, poll-only backends
  else:
      seed the ReadySet directly from `bits`
  evaluate Trigger
  dispatch
```

Poll-only backends (XRCE-DDS-Client, bare-metal smoltcp, uORB) never
supply a token, always report UNKNOWN, and get **exactly today's behaviour**.
This is the same graceful-degradation contract `supports_wake_callback` already
states, one level finer.

### 4.4 Triggers still work

`Trigger::AllOf` / `AnyOf` evaluate against a `ReadinessSnapshot`. Seeding that
snapshot from `bits` rather than from 64 `has_data` calls changes where the
bits come from, not what they mean. `Trigger::Custom` is unaffected — it
receives the same bool array.

One semantic caveat, and it is the reason `has_data` cannot simply be deleted:
a bit records that data *arrived*, while `has_data` reports that data is
*present now*. A callback that drains its queue and a concurrent arrival can
disagree. The rule that keeps them consistent: **the bit is cleared when the
callback is dispatched, not when the queue is drained**, and a spurious
dispatch to an empty queue is permitted — callbacks already tolerate this,
because the idempotent-insert contract has always allowed one ready bit to
stand for any number of queued messages.

### 4.5 The token also names the owner

Several executors can share one session (a tiered image's spawned tiers; C
executors over one support context). RFC-0106 D6 wakes them all through a
fan-out, because the session callback cannot say whose data arrived. The
per-entity slot's `ctx` is the REGISTERING executor's, so an arrival wakes
that executor alone. The fan-out stays only for backends whose slot is NULL.
Measured on a native host (phase-436 step 5): 6.8-9.4 us of CPU per spurious
wake, about 0.8 % of a core per idle executor at 1 kHz.

## 5. This is the vehicle for the dead 110.A traits

`Activator` and `Dispatcher` (`executor/activator.rs`, `executor/dispatcher.rs`)
were defined by phase 110.A with the note "110.A.b rewires `spin_once` to drive
activation through this trait instead of the inline bitmap scan". 110.A.b never
landed. Both traits carry `#[allow(dead_code)]`, `ActivatorCtx` is a
`PhantomData` shell, and grep finds no reference outside their own files and
doc comments.

They describe precisely the seam this RFC needs: something that decides *what
should fire* separately from something that *fires it*. Two activators —
`ScanActivator` (today's loop) and `PushedActivator` (read the bitmap) — is the
natural shape, selected per session by whether a token was supplied.

So: land 110.A.b as part of this, or delete the traits. Keeping an abstraction
that documents an architecture the code does not have is the worse of the three
options.

## 6. Alternatives considered

**Zephyr `k_poll`.** The native answer: one blocking call over an array of
`k_poll_event`. It is genuinely better than a single semaphore *and* it is
Zephyr-specific, while the platform ABI is deliberately portable
([RFC-0076](0076-platform-abi-ask-do-not-assume.md)). If wait-on-many is wanted,
it belongs in the ABI as a primitive that Zephyr implements with `k_poll`,
FreeRTOS with a queue set, and POSIX with `ppoll` — not as a direct call.
Orthogonal to this RFC and compatible with it.

**An `rmw_wait` equivalent.** What rclc leans on: push multiplexing into the
rmw layer and let it return the ready set. It is why micro-ROS's executor can
avoid this scan. It is also a much larger surface to specify across four
backends, and it moves the problem rather than removing it — someone still
has to attribute the arrival. The token is the small half of that idea.

**Per-entity condition variables.** One wait primitive per entity. Costs a
kernel object per entity on targets counting bytes, and does not compose with
`AllOf`/`AnyOf` triggers.

**Leave it alone.** Defensible today: N is 64 and a `has_data` call is cheap.
It stops being defensible when `MAX_CALLBACK_SLOTS` grows past the `u64` (the
`BitSet<N>` rewrite is already anticipated in `ready_set/mod.rs`), and it is
already wrong in principle on an image whose whole purpose is bounded latency.

## 7. Prior art

- **Embassy** — `Waker` per task, intrusive lists, no allocation.
  `WakerRegistration` inside each peripheral driver is this design applied one
  layer lower, and is the pattern to copy for a UART RX ring
  ([issue 0852](../issues/archived/0852-zephyr-serial-rx-is-polled-and-overruns.md)).
- **RTIC** — tasks *are* NVIC priorities; readiness is dispatched by hardware.
  Not adoptable wholesale under ROS semantics, but it is the existence proof
  that the scan is not necessary.
- **rmw_zenoh (full ROS 2)** — callback per entity plus a condition variable.
  Same backend as ours, so the entity-level callbacks this RFC needs are known
  to be expressible in zenoh-pico.
- **rclc executor (micro-ROS)** — has the same O(N) handle scan, and reaches
  for `rmw_wait` rather than a token. Worth reading for what it does *not*
  solve.
- **`embedded-io-async`** — the trait shape for a wakeable byte source.

## 8. Cost

| | |
| --- | --- |
| ABI | three NULL-able vtable slots, upstream's names and signature (§4.2, landed); `set_wake_callback` kept for backends that leave them NULL |
| executor state | one `AtomicU64` in `WakeCtx` (8 B) |
| hot path added | one `fetch_or` per arrival |
| hot path removed | up to 64 indirect calls per spin |
| backends touched | zenoh (through the Rust adapter, landed) and Cyclone (`on_data_available` receives the reader; still NULL); XRCE, uORB and bare metal leave the slots NULL and keep today's behaviour |
| risk | the arrived-vs-present semantics of §4.4 |

## 9. Open questions

1. Should the token be a `DescIdx` (dense, `u64`-indexable, executor-minted) or
   an opaque backend handle the executor maps? Dense is cheaper and bounded by
   `MAX_CALLBACK_SLOTS`; opaque survives the `BitSet<N>` widening better.
2. Does anything downstream depend on `has_data` being called every spin as a
   side effect — polling a backend to make it progress? If a backend needs to
   be poked to advance, the pushed path must poke it too.
3. `ready_bits` is `u64`; the widening to `BitSet<N>` needs a lock-free
   multi-word set-bit, or a per-word atomic array. Worth settling before the
   token type is fixed, since (1) depends on it.
