---
id: 792
title: "Twelve C lifecycle entry points cannot be called from C — cbindgen emits
  `Option_LifecycleCallbackFnCtx` as an opaque forward declaration and passes it
  by value; and `nros_lifecycle_init` discards the node it is given"
status: open
type: bug
area: api, lifecycle
related: [rfc-0036, phase-379, issue-0788]
---

## Problem 1 — twelve entry points are uncallable from C

`packages/api/nros-c/include/nros/nros_generated.h:769`:

```c
typedef struct Option_LifecycleCallbackFnCtx Option_LifecycleCallbackFnCtx;
```

That is an **opaque forward declaration** — a struct with no definition. It is
then passed **by value** to all six `nros_lifecycle_register_on_*` and all six
`nros_executor_lifecycle_register_on_*`. A C caller cannot form the argument:

```c
#include "nros/nros.h"
void probe(struct nros_lifecycle_state_machine_t *sm,
           struct Option_LifecycleCallbackFnCtx cb, void *ctx) {
    nros_lifecycle_register_on_configure(sm, cb, ctx);
}
```

```
error: variable has incomplete type 'struct Option_LifecycleCallbackFnCtx'
note: forward declaration of 'struct Option_LifecycleCallbackFnCtx'
```

So the entire C lifecycle callback surface is dead. Nothing in the tree calls it,
which is why it stayed latent: every in-tree caller is Rust, or goes through the
C++ shim.

**The ABI itself is fine.** A nullable `extern "C" fn` is one pointer. And **our
own C++ shim already spells it correctly** — `nros_cpp_lifecycle_callback_t` in
`packages/api/nros-cpp/src/lifecycle_shim.rs` is a plain fn-pointer typedef. The
C side is the one that went through cbindgen with an `Option<...>` in the
signature and got a Rust type name plus an incomplete type.

This is also why phase 379's first report saw five `lifecycle_*` rows it could
not explain: the arity comparison flagged them, and the real defect was one level
below the arity.

### Fix shape

A named typedef and a plain nullable pointer:

```c
typedef nros_lifecycle_ret_t (*nros_lifecycle_callback_t)(void *ctx);
nros_ret_t nros_lifecycle_register_on_configure(
    struct nros_lifecycle_state_machine_t *sm,
    nros_lifecycle_callback_t cb, void *ctx);
```

Rust-side, that means not putting `Option<LifecycleCallbackFnCtx>` in an
`extern "C"` signature — the same discipline RFC-0054 already applies to the
committed bindgen output for the RMW/platform ABI.

## Problem 2 — `nros_lifecycle_init` accepts a node and throws it away

`packages/api/nros-c/src/lifecycle.rs:119-136`:

```rust
pub unsafe extern "C" fn nros_lifecycle_init(
    sm: *mut nros_lifecycle_state_machine_t,
    node: *const nros_node_t,
) -> nros_ret_t {
    if sm.is_null() || node.is_null() { return NROS_RET_INVALID_ARGUMENT; }
    ...
    (*slot).write(LifecyclePollingNodeCtx::new());   // `node` never used again
```

`node` is NULL-checked and never stored. So the standalone
`nros_lifecycle_state_machine_t` family cannot reach its node and therefore
cannot register the REP-2002 services — only the executor-scoped family can.
`nros_make_node_a_lifecycle_node` is a documented alias for this entry point, and
today **it does not make a node a lifecycle node**.

## What is NOT wrong, and should stop being written down

RFC-0036 line 92 says "No lifecycle-node graph — a simplified state model for
embedded executors". That is stale. We implement the full REP-2002 machine — five
primary states including `ErrorProcessing`, all six hooks, `on_error` defaulting
to `Failure` — and register **five** services, more than rclc's three and matching
rclcpp, so `ros2 lifecycle set|get|list` drives an embedded node today.

The one real omission is the **`~/transition_event` publisher**: we announce no
transitions, so a supervisor must poll `~/get_state`. rclc threads a
`bool publish_update` through every transition for exactly this, which is why
`lifecycle_change_state` has one parameter fewer here. That is the other end of
the `cpp:LifecyclePublisher` gap recorded in the pubsub stage.

RFC-0036 should be corrected to name the transition_event publisher and the
managed-entity protocol as what is missing, rather than implying the state model
is simplified.

## Evidence

* the compile error above, reproduced with `clang -std=c11 -fsyntax-only
  -DNROS_PLATFORM_NUTTX -Ipackages/api/nros-c/include`
* `packages/api/nros-c/src/lifecycle.rs:119-136`
* `packages/api/nros-cpp/src/lifecycle_shim.rs` — the correct spelling, one
  language over
* `scripts/api-parity.py --topic lifecycle` and
  `docs/reference/api-parity-ledger/lifecycle.json`
