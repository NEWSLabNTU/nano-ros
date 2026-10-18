---
id: 1940
title: "Entity-scoped wake registration is a no-op through `nros-rmw-cffi`: `rclc_polling_*_set_wake_callback`, the C++ twins and `recv().await`'s waker never reach a backend"
status: resolved
type: bug
severity: medium
area: [rmw, api, executor]
related: [phase-397, rfc-0084, 0960]
found: 2026-10-17
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

This is the same slot [phase-397](../roadmap/phase-397-pushed-readiness-executor.md)'s
W1 needs: the executor registers each entity's `(wake context, DescIdx)` there.
So land this first, as phase-397 W1.

Acceptance:

- A C test sets `rclc_polling_subscription_set_wake_callback` and observes the
  callback fire on an arrival, against zenoh.
- A backend without the slot makes the call return an error, not `OK`.

## Resolution (2026-10-18, phase-397 W1)

The vtable gained upstream's per-entity trio,
`{subscription,service,client}_set_on_new_*_callback`, in upstream's
signature (`rmw_event_callback_t`), rather than the `*_set_wake(…, token)`
shape the fix direction above proposed: `user_data` is already per entity,
so a token argument added nothing. Issue 0960 had declined the trio on RAM
grounds; the header keeps that argument by making `number_of_events` a lower
bound, so no backend tracks per-entity depth.

- `register_waker` on all three entity traits returns `bool`, `false` by
  default. `CffiSubscription` / `CffiService` / `CffiClient` keep the waker in
  a per-entity slot and forward it through the new slots
  (`nros-rmw-cffi/src/entity_wake.rs`).
- The Rust adapter fills the slots for every Rust backend, zenoh included. Its
  waker re-arms itself after each wake, because zenoh's `AtomicWaker` gives the
  waker up and the ABI says a registration persists. The zenoh client now keeps
  a waker registered before any request exists.
- The C and C++ setters return `RCL_RET_UNSUPPORTED` / `NROS_CPP_RET_UNSUPPORTED`
  when the backend cannot wake. That covers the three entity setters named
  here and the seven C++ and C action-channel setters, which were inert the
  same way and are the same class.
- Cyclone, XRCE and uORB leave the slots NULL. Cyclone's
  `on_data_available` half moved to phase-397 W2.

Acceptance, as run:
- `just check c` runs `tests/run/entity_wake_callback.c` against the stub
  backend: each kind's callback fires on each of three arrivals, and with the
  slots answering UNSUPPORTED each setter returns `RCL_RET_UNSUPPORTED`.
- `nros-rmw-zenoh`'s `cffi_smoke::cffi_subscription_waker_fires_on_each_arrival`
  registers a waker once, over a real router, and sees it woken on at least
  three arrivals. It now runs in `test-all`; before this, `cffi_smoke` was
  compiled without `platform-posix` in every lane and ran nowhere. With the
  adapter's re-arm removed it reads 1 wake and fails.
- No XRCE run was done for "reports unsupported". XRCE's NULL slot takes the
  same code path the stub's UNSUPPORTED arm drives.
