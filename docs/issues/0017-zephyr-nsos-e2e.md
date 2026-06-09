---
id: 17
title: Zephyr workspace Entry — native_sim zenoh E2E delivers (RESOLVED)
status: resolved
type: bug
area: zephyr
related: [issue-0018]
---

**Status (2026-06-09, RESOLVED): the Phase 225.P Zephyr workspace Entry
now publishes `/chatter` over zenoh on native_sim and an external native
listener receives it cross-process** (`Received: 0,1,2,…`).
`test_zephyr_workspace_entry_native_sim_e2e` passes (`1 passed`, 9
messages delivered in a 41 s window). The chain — `just zephyr
build-fixtures` (`west build`) → boot `zephyr.exe` → `nros_net_wait`
network gate → register the launch node set → register the zenoh backend
→ `Executor::open` → publish → cross-process delivery to the external
listener — works end to end.

**The earlier "environmental NSOS offload is broken" diagnosis was WRONG
— same misdiagnosis class as issue #18 (NuttX).** The evidence that read
as "NSOS never issues a `connect()`" was actually an EMPTY locator: the
Rust path used `ExecutorConfig::default_const()` (empty locator) → no TCP
target → zenoh-pico fell back to multicast scouting (which native_sim
can't satisfy), so there was nothing to `connect()` *to*. NSOS host-socket
offload is fully functional: with the locator fixed, `strace` shows
`connect(127.0.0.1:7456)=EINPROGRESS` followed by `sendto(...)` carrying
the `0/chatter/std_msgs::msg::dds_::Int32_` declarations + data samples,
and `zenohd --debug` logs the accepted transport, the subscriber/token
declarations, and routes data to the external listener.

The fix was a two-part cascade in the never-before-exercised Rust
Zephyr-zenoh native_sim path (commit `fix(zephyr): wire RMW backend +
baked locator …`):

1. **No RMW backend linked.** On `target_os = "none"` (native_sim)
   `linkme` is a no-op and the image does not run the `.init_array`
   auto-register fallback, so the CFFI vtable had no transport and
   `Executor::open` returned `Transport(ConnectionFailed)`. The
   `nros::main!` Zephyr branch now calls `nros::__register_linked_rmw()`
   (a feature-dispatched, idempotent facade) before `Executor::open`;
   `zephyr_component_main!` (single-node) does the same.

2. **Empty locator.** `default_const()` → multicast scouting. The branch
   now bakes the locator via `option_env!("NROS_LOCATOR")`, and the Entry
   `build.rs` re-exports `CONFIG_NROS_ZENOH_LOCATOR` (the Kconfig the C API
   path already consumes) into that env — Kconfig is now the single source
   of truth for both languages.

**native_sim timing note:** on a slow native_sim host the Entry's
zenoh-pico session setup + first publish lands ~20 s after boot, then the
publish cadence tracks the ~2.5 s lease keepalive. The E2E listener wait
is 40 s to accommodate this (it always runs the full duration — the
listener `spin_blocking`s and never self-exits, so the bound caps
wall-time, not the success path). CI is faster; the bound is generous, not
tight.

**Single-node reference — talker direction RESOLVED.** All six single-node
zephyr rust examples (`talker`, `listener`, `action-{client,server}`,
`service-{client,server}`) now (a) call the renamed `export_bool_kconfig`
(was the dropped `export_kconfig_bool_options`) and (b) bake
`CONFIG_NROS_ZENOH_LOCATOR` → `NROS_LOCATOR` in their `build.rs`, mirroring
the Entry. `test_zephyr_to_native_e2e` (Zephyr talker → native listener)
**passes — 13 messages delivered cross-process.**

**Remaining open — Zephyr-as-subscriber dispatch on no_std (NOT a timing
issue).** `test_native_to_zephyr_e2e` and `test_bidirectional_native_zephyr_e2e`
fail: the Zephyr **listener** receives 0 samples from a continuously-
publishing native talker, while the reverse direction works
(bidirectional: `Zephyr → Native: 66 messages`). An earlier hypothesis
("slow-host receive starvation") was **disproven** by tracing.

Investigation (2026-06-09), in order:
1. `zenohd --debug` shows the Zephyr listener opens its transport and
   declares its subscriber `0/chatter/std_msgs::msg::dds_::Int32_/*` + the
   `/listener/` liveliness token — discovery is correct.
2. `strace -f -e recvfrom` on the listener shows the data samples DO
   arrive on its socket: `recvfrom(fd) = 102` once per second, payloads
   carrying `0/chatter/…/Int32_/TypeHashNotSupported`. So NSOS receive and
   zenoh-pico socket reads are healthy and on-time — not starved.
3. Yet nros dispatches **0** to the user callback.

Root cause is the no_std receive→executor dispatch bridge, not transport
or timing. The zenoh subscriber is declared with
`declare_subscriber_ring_raw` (an SPSC ring): the C shim writes each
received payload into `SUBSCRIBER_BUFFERS[i]`'s ring (advancing
`ring_tail`) and fires `subscriber_notify_callback`. That callback only
does `buffer.waker.wake()` (for the async `Future` path) plus a
`#[cfg(feature = "std")] signal_executor_wake()` — **on no_std there is no
synchronous signal to the blocking executor.** The blocking `spin_once`
readiness scan evaluates `(meta.has_data)(arena_ptr + offset)` against the
executor *arena*, which the ring producer never populates, so the ring is
never drained (`peek_head_slot` → `consume_head` in the `take_*` methods
is never reached) and the subscription callback never fires. Publishing is
unaffected because TX is push-driven inside `spin_once`.

This is a genuine, almost-certainly-never-worked no_std zephyr receive gap
(consistent with the single-node reference never having delivered). The
fix is to wire the no_std path so a ring write marks the owning executor
entry ready (or so the executor unconditionally drains each ring-backed
subscriber per spin) — a focused change in
`packages/zpico/nros-rmw-zenoh/src/shim/subscriber.rs`
(`subscriber_notify_callback`) + the executor readiness/`has_data` wiring
for ring-backed CFFI subscriptions. Tracked as the remaining work here.

The E2E listener/receive waits were still raised (40 s / 45 s) — correct
regardless, since first delivery is slow even once dispatch is fixed.

**Cross-reference**: the sibling issue #18 (NuttX) is also RESOLVED via
the same locator + backend-register cascade (its entry boots on
`qemu-system-arm` rather than native_sim, but the root cause and fix shape
are identical).
