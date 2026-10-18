---
id: 1940
title: "Entity-scoped wake registration is a no-op through `nros-rmw-cffi`: `rclc_polling_*_set_wake_callback`, the C++ twins and `recv().await`'s waker never reach a backend"
status: resolved
type: bug
severity: medium
area: [rmw, api, executor]
related: [phase-397, rfc-0084, 0960]
found: 2026-10-17
resolved: 2026-10-18
---

## What happens

Every way to ask for a wake when ONE entity has data ends in
`nros_rmw::Subscription::register_waker` (or the `ServiceTrait` /
`ClientTrait` twin), and in a build with `rmw-cffi` that call does nothing:

- C: `rclc_polling_subscription_set_wake_callback`,
  `rclc_polling_service_set_wake_callback` and
  `rclc_polling_client_set_wake_callback` (`nros-c/src/subscription.rs`,
  `service.rs`) build a `Waker` from the caller's `(cb, ctx)` and call
  `raw.register_waker(&waker)`. They return `RCL_RET_OK`.
- C++: `nros_cpp_subscription_set_wake_callback`,
  `nros_cpp_service_server_set_wake_callback` and
  `nros_cpp_service_client_set_wake_callback` (`nros-cpp/src/action.rs`) go
  through the same `register_waker`.
- Rust: `Subscription::recv().await` (`nros-node/src/executor/handles.rs`)
  registers `cx.waker()` and returns `Pending`, trusting the backend to wake
  it.

## Cause

Found by reading the code on 2026-10-17, while reviewing phase-397. Not yet
reproduced by a run.

- With `rmw-cffi`, nros-node's `ConcreteSession` is
  `nros_rmw_cffi::CffiSession` (`nros-node/src/session.rs`), whose handles are
  `CffiSubscription`, `CffiService` and `CffiClient`.
- None of the three overrides `register_waker`, so each inherits the trait
  default, `fn register_waker(&self, _waker: &Waker) {}`
  (`nros-rmw/src/traits.rs`).
- Nothing could forward it anyway. The C ABI the backends implement,
  `nros-rmw-abi/include/nros/rmw_vtable.h`, has only the session-scoped
  `set_wake_callback`; there is no per-entity wake slot.

Only zenoh's native Rust shim implements `register_waker`
(`nros-rmw-zenoh/src/shim/subscriber.rs`, `service.rs`): its arrival hook
calls `buffer.waker.wake()`. Since phase-128.C, an executor reaches zenoh
through the cffi vtable, not through that shim's trait impl.

## Consequences

- A C or C++ caller that sets a wake callback gets `OK`, and the callback
  never fires. No test sets one: none of the three C functions, nor the C++
  twins, has a caller under `packages/testing`, `packages/api/*/tests` or
  `examples/`.
- `recv().await` is woken only if something else re-polls its task. Under
  `block_on` over a spin loop that happens every spin, so it works by
  polling. Under an executor that sleeps until woken, it would wait forever.
  Not measured.

## Fix direction

Add a per-entity wake slot to the RMW vtable: a subscription, service or
client takes `(cb, ctx, token)`. `CffiSubscription` / `CffiService` /
`CffiClient` forward `register_waker` to it, and a backend that leaves it NULL
reports "unsupported", so the call can say so rather than return `OK`.

This is the same slot [phase-397](../../roadmap/phase-397-pushed-readiness-executor.md)'s
W1 needs: the executor registers each entity's `(wake context, DescIdx)` there.
So land this first, as phase-397 W1.

Acceptance:

- A C test sets `rclc_polling_subscription_set_wake_callback` and observes the
  callback fire on an arrival, against zenoh.
- A backend without the slot makes the call return an error, not `OK`.

## Resolution (2026-10-18)

Confirmed by reading before fixing: none of the three cffi handles overrode
`register_waker`, and `rmw_vtable.h` had no per-entity slot to forward to.
Fixed as phase-397 W1 describes, for the whole class.

**The ABI.** Three optional slots, appended (so positional C++ initialisers —
uORB, Cyclone — still compile and leave them NULL): `subscription_set_wake`,
`service_set_wake`, `client_set_wake`, each
`(entity, rmw_entity_wake_callback_t cb, void *ctx, uint32_t token)`, plus
`NROS_RMW_WAKE_TOKEN_UNKNOWN`. Bindings regenerated
(`scripts/gen-abi-bindings.sh`), `rmw-abi-shape` declares them as RTOS
additions, RFC-0035's slot table regenerated. They are NOT upstream's
`*_set_on_new_*_callback`, which issue 0960 declined for its count; these are
edges.

**One shared helper, both directions** — `nros-rmw-cffi/src/entity_wake.rs`:

- `EntityWake` (Rust `Waker` -> C triple) backs `register_waker` on
  `CffiSubscription`, `CffiService` and `CffiClient`. A waker built by
  `c_waker::make_waker` (every C and C++ caller) is forwarded with no storage:
  `c_waker` moved from `nros-node` to `nros-rmw` (re-exported, so
  `nros_node::c_waker` still names it) and gained `state_of`, so the cffi crate
  can recognise one. Any other waker (`recv().await`, the arena flag wakers) is
  carried in a boxed `atomic-waker`-protocol cell under `alloc`; without
  `alloc` that case reports unsupported instead of pretending. The cell is
  freed only after `destroy_*` returned OK.
- `AdapterWake` (C triple -> Rust `Waker`) lets `RustBackendAdapter` fill the
  three slots for every Rust backend. Each entity is stored as
  `Hosted<T> { handle, wake }` (`repr(C)`, handle first, so every existing
  `backend_data as *mut T` cast still holds). The trait's wakers are
  one-shot, so the adapter re-arms on every take (`take`, `take_sequence`,
  `process_raw_in_place`, `take_request`, `take_response`) and fires the
  callback itself if data is already waiting — the window between the last
  wake and the re-registration.

**Unsupported is reported.** `register_waker` returns `bool` now (trait
default `false`). `rclc_polling_{subscription,service,client}_set_wake_callback`,
the seven action-channel wake setters (C), and the C++ `nros_cpp_*` twins
return `RCL_RET_UNSUPPORTED` / `NROS_CPP_RET_UNSUPPORTED` when it is `false`,
not `OK`. A NULL slot (XRCE, uORB, Cyclone today) and a backend handle on the
trait default both answer `false`.

**Zenoh.** Its shim's three `register_waker`s return `true`. The client's only
reached reply slots outstanding at the moment of registration, so a waker
installed once (the C path) never met a request sent later; it now keeps the
waker and registers it on every request it tracks.

**Tests.**

- `packages/rmw/cffi/tests/entity_wake.rs`: a C-shaped stub backend records
  what each `*_set_wake` slot is handed and plays the arrival, for all three
  kinds and both waker kinds; a poll-only backend answers `false`; a Rust
  backend behind the adapter with a ONE-SHOT waker proves the callback fires,
  survives each take, and fires on re-arm when a sample landed while the waker
  was spent; a Rust backend on the trait default answers `false` through the
  adapter.
- `packages/api/nros-c/tests/run/polling_entity_wake.c` (in `just check c`):
  `rclc_polling_subscription_set_wake_callback` against the C stub — the
  callback is installed and fires on each arrival, and a refusing backend
  makes the call return `RCL_RET_UNSUPPORTED`.
- `packages/rmw/zenoh/nros-rmw-zenoh/tests/cffi_entity_wake.rs`
  `cffi_subscription_wake_fires_on_a_live_arrival`: a `c_waker` on a cffi
  zenoh subscription fires on a live arrival through `rmw_zenohd`, with no take
  before it, and again after a drain.

Mutation-checked: with the cffi handles' forwarding disabled, the C-stub and
adapter tests fail; with the adapter's re-arm-fire removed, the window
assertion fails.

**Not in this issue.** Cyclone can attribute an arrival (its
`on_data_available` receives the reader) but leaves the slots NULL, so it
answers unsupported; filling them is phase-397 W1's Cyclone half. The executor
registering its own wake context per entity is phase-397 W2.

