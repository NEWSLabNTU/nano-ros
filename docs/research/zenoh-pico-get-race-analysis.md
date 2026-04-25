# zenoh-pico `z_get` race analysis

**Date**: 2026-04-25
**Context**: NuttX Rust rtos_e2e service/action cases flake on cold
boot with `Application error: ServiceRequestFailed` on the first
`client.call()`. The shim-side fix in
`nros-rmw-zenoh/src/shim/service.rs::send_request_raw` retries
`zpico_get_start` for up to 800 ms to mask the races; this note
documents what those races actually are in zenoh-pico, what the
upstream design choices look like, and what (if any) upstream patch
would let us delete the shim retry loop.

## Reproduction

- NuttX QEMU ARM cold boot; server and client launched in parallel.
- Server prints `Waiting for requests` within ~10 s. Client boots and
  calls `client.call(&req)` as soon as `create_client(...)` returns.
- `call()` → `Promise::send_request_raw` → `zpico_get_start` →
  `z_get` → `_z_query` returns non-zero → shim maps to
  `ZPICO_ERR_GENERIC` → `send_request_raw` bubbles
  `TransportError` → `NodeError::ServiceRequestFailed`.
- Flake rate ~33 % on `armv7a-nuttx-eabihf` under QEMU slirp even
  after my 800 ms retry + `thread::sleep(5ms)` loop.

## What the user sees

```
Creating service client: /add_two_ints (AddTwoInts)
Client ready

Calling: 5 + 3 = ?
Application error: ServiceRequestFailed
```

## Call path

```
z_open  (api.c:735)
  └─ _z_open                                (net/session.c:179)
       └─ _z_open_inner                     (net/session.c:161)
            └─ _z_new_transport             (transport/manager.c:146)
                 └─ _z_new_transport_client (transport/manager.c:26)
                      └─ _z_unicast_open_client       (transport/unicast/transport.c:293)
                           └─ _z_unicast_handshake_open  (transport/unicast/transport.c:104)
                                ├─ _z_link_send_t_msg(INIT_SYN)
                                ├─ _z_link_recv_t_msg(INIT_ACK)   ← BLOCKING
                                ├─ _z_link_send_t_msg(OPEN_SYN)
                                └─ _z_link_recv_t_msg(OPEN_ACK)   ← BLOCKING

z_get   (api.c:1413)
  └─ _z_query                          (net/primitives.c:485)
       ├─ _z_keyexpr_copy
       ├─ _z_get_query_id              ← unsynchronized ++
       ├─ _z_session_mutex_lock
       ├─ _z_unsafe_register_pending_query
       ├─ build _z_pending_query_t entry
       ├─ _z_session_mutex_unlock
       └─ _z_send_n_msg                ← TX-mutex + congestion-control gated
```

## Is `z_open` non-blocking by design? — **No, it blocks the full handshake.**

I originally hypothesized `z_open` returned immediately after `connect(2)`.
That's wrong. `_z_unicast_handshake_open` (transport.c:104) runs the
**full four-message Zenoh transport handshake synchronously** before
`z_open` returns:

```
client                                       router
  ──────── INIT(Syn)        ─────────────►
  ◄─────── INIT(Ack)        (blocking recv)
  ──────── OPEN(Syn)        ─────────────►
  ◄─────── OPEN(Ack)        (blocking recv)
```

So by the time `z_open` returns, the unicast transport peer is in
`_Z_LINK_STATE_ESTABLISHED`. Subsequent `z_get` / `z_put` see a
fully-formed transport. **Race 3 (handshake-not-yet-complete) from
the previous draft of this note is not happening.**

That changes the picture: this isn't a "z_open should block longer"
problem, because z_open already does. The flake must come from
something downstream of the handshake.

## What does happen during the cold-boot window

Re-reading `zpico_open` (zpico.c:600):

```c
int open_ret = z_open(&g_session, ...);            // blocks: TCP + handshake
if (open_ret < 0) return ZPICO_ERR_SESSION;
zp_start_read_task(...);                            // spawn pthread
zp_start_lease_task(...);                           // spawn pthread
g_session_open = true;
return ZPICO_OK;
```

After `zpico_open` returns, all four conditions hold:
1. TCP socket connected.
2. Transport handshake complete.
3. Read task running (pthread on multi-threaded backends).
4. Lease task running.

Then the user-thread does:
1. `create_client` — declares a liveliness token + key resource. These
   are TX-only declarations sent to the router. The router doesn't
   acknowledge them on the wire (declarations are fire-and-forget in
   the protocol).
2. `client.call(req)` → `z_get` → `_z_query` → `_z_send_n_msg`.

So the question becomes: at step 2, the transport is established, the
TX path is functional, declarations have been sent — what makes the
first `z_get` fail?

Three remaining hypotheses I haven't ruled out:

### H1 — `_z_send_n_msg` lock contention on first send

The TX mutex (`ztc->_mutex_tx`) is acquired by both the user thread
(via `_z_query` → `_z_send_n_msg`) and any internal task that needs to
emit (lease task sending KEEPALIVE, read task pumping replies through
local-queryable delivery). Default congestion control for queries is
not declared at the call site; if it falls back to
`Z_CONGESTION_CONTROL_DROP`, `_z_transport_tx_mutex_lock(ztc, false)`
calls `_z_mutex_try_lock`. **A failed try_lock returns
`_Z_ERR_SYSTEM_TASK_FAILED` and the message is dropped silently
("Dropping zenoh message because of congestion control").** That
maps to a non-zero return from `_z_query`, which is exactly what
the shim sees.

The lease task on NuttX runs at default 1 s cadence and holds the TX
mutex briefly for KEEPALIVE bursts. If the user's first `z_get`
happens to land during a KEEPALIVE burst, the try_lock fails. By the
second call (a few ms later) the lease task has released the mutex
and the call succeeds.

This matches the observed pattern: 1-in-3 flake rate, first call
only, second call always succeeds.

**Verification needed**: log the actual `ret` from `_z_send_n_msg`
on the failing path to confirm it's
`_Z_ERR_SYSTEM_TASK_FAILED` ("congestion control drop").

### H2 — declaration ordering vs. router state propagation

Even though the TX path is open, the router is forwarding declarations
across sessions. If the **server's** queryable declaration hasn't
reached the router by the time the **client's** query arrives, the
router replies with FINAL+no-replies. That isn't a `_z_query` failure
though — the client's `_z_query` would succeed, the timeout would
fire later, and the test would see "0 responses". That's the C variant
on NuttX (which uses 15-s wall-clock budgets, masks this), not the
Rust failure mode (immediate `ServiceRequestFailed`).

So H2 explains the C flake but not the Rust one.

### H3 — `_z_get_query_id` stores `_query_id++` outside `_mutex_inner`

```c
// session/query.c:61
_z_zint_t _z_get_query_id(_z_session_t *zn) { return zn->_query_id++; }
```

Read-modify-write outside any mutex. zenoh-pico's user-thread API has
a single user thread by convention, so an isolated client typically
won't tear this. **But on NuttX with `Z_FEATURE_MULTI_THREAD=1`, the
read task can dispatch a `_z_trigger_query_reply_*` callback that —
in our case — runs `pending_get_reply_handler` on the user thread's
data path.** That handler doesn't call `_z_get_query_id`, so this
race does not directly explain the flake. Still a code-correctness
smell worth fixing under the same patch series.

## Three race classes (revised)

### Race 1 — `_z_get_query_id` is unsynchronized

**Severity**: Low. Doesn't cause the observed flake; cosmetic + future
proofing for any caller introducing a second user thread.

**Fix** (minimal): wrap the `++` in `_z_session_mutex` or switch the
field to `atomic_size_t` behind `Z_FEATURE_MULTI_THREAD`.

### Race 2 — `_z_unsafe_register_pending_query` can reject a fresh ID

`_z_get_query_id` runs *outside* the session mutex; the registration
runs *inside*. A theoretical window exists where two user threads both
read the same counter value, the first registers, the second fails
with `_Z_ERR_ENTITY_DECLARATION_FAILED`. Single-user-thread programs
don't hit it.

**Severity**: Low. Same applies as Race 1. Not the observed flake.

**Fix**: widen the session mutex to cover `_z_get_query_id` +
`_z_unsafe_register_pending_query` as one atomic section.

### Race 3 — `_z_send_n_msg` drops on TX-mutex try-lock failure *(most likely root cause)*

**Severity**: High — best candidate for the NuttX Rust flake.

**Fix options**:

1. **At the `_z_query` call site**: pass
   `Z_CONGESTION_CONTROL_BLOCK` for query traffic instead of
   inheriting the call-site default. Query messages are inherently
   request-reply and the user is already blocking on the reply, so
   blocking on the TX mutex for a few μs isn't a regression.

2. **At `_z_transport_tx_mutex_lock`**: a *brief* internal retry
   when `try_lock` fails on a "block-fallback" path, so that
   foreground sends compete fairly with the lease task's KEEPALIVE
   bursts. Riskier — changes mutex semantics globally.

3. **At the shim**: keep the user-thread retry loop (what I've
   shipped). Trade-off: caller burns CPU spinning vs. zenoh-pico
   transparently doing the right thing.

Option 1 is the surgical upstream fix.

## Do we still need a `z_session_wait_established` API?

**No.** `z_open` already blocks until the transport handshake is
complete. The pattern I sketched earlier
(`z_session_wait_established`) is *redundant* given the existing
synchronous handshake. Walk it back from the upstream proposal.

## Revised recommendation for nano-ros

1. Keep the current 800 ms retry loop in
   `nros-rmw-zenoh/src/shim/service.rs` as the user-side workaround
   for now.
2. Submit one upstream patch: `_z_query` should pass
   `Z_CONGESTION_CONTROL_BLOCK` to `_z_send_n_msg` (Race 3, fix 1).
   Smallest possible diff; explicit semantic that "queries don't
   silently drop".
3. Race 1 and Race 2 are cosmetic cleanups — bundle them with the
   same PR if convenient, but they're not load-bearing.
4. Once the upstream change lands and the zenoh-pico submodule is
   bumped, re-run the NuttX rtos_e2e service/action cases in a tight
   loop to confirm the flake is gone, then delete the 800 ms retry
   loop.

## Files referenced

- `packages/zpico/zpico-sys/zenoh-pico/src/api/api.c:735`
  — `z_open` user-facing entry (blocks on handshake, *not* fire-and-forget).
- `packages/zpico/zpico-sys/zenoh-pico/src/transport/unicast/transport.c:104`
  — `_z_unicast_handshake_open` (the synchronous four-message handshake).
- `packages/zpico/zpico-sys/zenoh-pico/src/net/primitives.c:485`
  — `_z_query` and the `_z_send_n_msg` call site (Race 3).
- `packages/zpico/zpico-sys/zenoh-pico/src/transport/common/tx.c:299`
  — `_z_transport_tx_send_n_msg` and the
  `_z_transport_tx_mutex_lock(ztc, cong_ctrl == Z_CONGESTION_CONTROL_BLOCK)`
  switch.
- `packages/zpico/zpico-sys/zenoh-pico/src/session/query.c:61`
  — `_z_get_query_id` (Race 1) and register helpers (Race 2).
- `packages/zpico/zpico-sys/c/zpico/zpico.c:600`
  — our `zpico_open` (calls `z_open` then spawns lease/read tasks).
- `packages/zpico/nros-rmw-zenoh/src/shim/service.rs:459`
  — our 800 ms retry loop.
