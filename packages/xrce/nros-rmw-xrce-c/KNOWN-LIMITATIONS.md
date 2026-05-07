# nros-rmw-xrce-c — known limitations

Phase 115.K.2.0 through 115.K.2.4 ship a deliberately minimal
re-implementation of the Rust `nros-rmw-xrce` backend in pure C99.
Each gap below is the difference vs. the Rust ground truth at
`packages/xrce/nros-rmw-xrce/src/lib.rs`. Items are roughly ordered
by likely impact on real workloads.

## QoS XML profile path missing

The Rust impl falls back to `uxr_buffer_create_*_xml` whenever the
QoS profile uses extended policies the binary `uxrQoS_t` can't
represent (deadline, lifespan, manual liveliness, lease). The C
backend keeps the binary path only — extended-policy QoS lands
identically to default-QoS at the agent. Phase 108.C.xrce.3 in the
Rust impl is not ported.

**Workaround:** stick to reliability + durability (V/TL) + history +
depth.

**Tracked:** TODO 115.K.2.x in `src/publisher.c` /
`src/subscriber.c` / `src/service.c` (search for `uxr_buffer_create_topic_bin`).

## Deadline events absent

The Rust impl emulates `OfferedDeadlineMissed` and
`RequestedDeadlineMissed` via a shim-side platform clock. The C
backend has no deadline tracking; `register_subscriber_event` /
`register_publisher_event` aren't even implemented (the vtable
entries stay `NULL`).

**Workaround:** implement deadline checks at the application layer
or use the `nros-rmw-xrce` Rust backend for deadline events.

## Async wakers absent

Subscribers / service clients in the Rust impl carry an
`AtomicWaker` so executor-driven code can `await` on a poll. The C
backend's `try_recv_raw` is purely poll-based — callers spin or
sleep externally.

**Workaround:** poll from the application's main loop. The bounded
busy-wait inside `xrce_service_call_raw` (5000 ms total, 50 ms per
iteration) is the pattern to mirror.

## Fragmented publish path absent

`xrce_publisher_publish_raw` only uses the fast path
(`uxr_buffer_topic`). Payloads larger than a single stream slot
return `NROS_RMW_RET_MESSAGE_TOO_LARGE`. The Rust impl has a
fragmented fallback through `uxr_prepare_output_stream_fragmented`.

**Workaround:** keep messages small, or split at the application
layer.

**Tracked:** TODO 115.K.2.x in `src/publisher.c`.

## Single-slot ringbuffer overflow drops

Each subscriber / service-server / service-client holds one slot
of `XRCE_BUFFER_SIZE` (1024) bytes. Concurrent inbound messages
during read flag `overflow` and the slot returns
`NROS_RMW_RET_MESSAGE_TOO_LARGE` on the next poll. The Rust impl
behaves the same way, but with an `AtomicBool` `locked` flag the
callback consults to avoid mid-read overwrites.

**Workaround:** poll fast enough to drain before the next inbound
arrives; bump `XRCE_BUFFER_SIZE` if topics are big.

## Runtime drain symbol not exported

`nros_rmw_xrce_init_custom_transport(framing)` is supposed to drain
whatever `nros_set_custom_transport()` (the `nros-c` C surface)
registered into the Rust runtime's slot, then install it on the
XRCE custom transport. That requires a C symbol like
`extern int32_t nros_rmw_take_custom_transport(struct nros_transport_ops *out);`
exposed from `nros-rmw-cffi`. As of Phase 115.K.2.4 that symbol
is not exported.

**Workaround:** pure-C clients use
`nros_rmw_xrce_set_custom_transport_ops(ops, framing)` directly,
which copies the user's vtable into backend-local storage. This
covers the practical case (a board that wants USB-CDC or BLE) at
the cost of bypassing the runtime's
`set_custom_transport`/`take_custom_transport` slot.

**To unblock:** add a C export to `nros-rmw-cffi` of the form

```c
/* Returns OK and writes ops if a transport was registered, NO_DATA
 * otherwise. Drains the slot. */
nros_rmw_ret_t nros_rmw_take_custom_transport(nros_transport_ops_t *out);
```

then update `nros_rmw_xrce_init_custom_transport` to call it. This
is a Phase 115.K.2.x follow-up — out of scope for K.2.4 per the
task description's "DON'T touch other phases" rule.

## Single session per process (transport-bridge slot)

The custom-transport bridge slot is file-scope inside
`src/transport_custom.c`. Mirrors the Rust impl's single-session
model, but we could in principle support multiple sessions by
moving the slot onto `xrce_session_state_t`. Phase 115.K.2.x.

## Session-key hash differs from Rust

The Rust impl uses djb2 for `hash_session_key`. The C backend uses
FNV-1a. Both are self-consistent; agent-side they don't matter
unless a Rust-driven and a C-driven session connect to the same
agent under the same node name. Unification before 115.K.2.5 flips
the C backend on under `-DNROS_C_RMW=xrce`.
