---
id: 792
title: "Twelve C lifecycle entry points cannot be called from C — cbindgen emits
  `Option_LifecycleCallbackFnCtx` as an opaque forward declaration and passes it
  by value; and `nros_lifecycle_init` discards the node it is given"
status: resolved
type: bug
area: api, lifecycle
related: [rfc-0036, phase-379, issue-0788]
---

## Problem 1 — twelve entry points are uncallable from C (FIXED 2026-08-25)

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

### Fixed

A LOCAL `Option<fn>` alias in `nros-c/src/lifecycle.rs`:

```rust
pub type nros_lifecycle_callback_t = Option<unsafe extern "C" fn(context: *mut c_void) -> u8>;
```

used at all fourteen sites. It renders as
`typedef uint8_t (*nros_lifecycle_callback_t)(void *context);`. ABI unchanged —
one pointer either way — so no call site needed adapting.

**Root cause, narrower than this issue first said:** `parse_deps = false` in
`cbindgen.toml`. cbindgen cannot resolve `LifecycleCallbackFnCtx` from
`nros-node`, so `Option<T>` degraded to an opaque struct. A local alias is what
the rest of the crate already does (`nros_guard_condition_callback_t`,
`nros_timer_callback_t`).

**A third defect fell out of the same root cause and would have made the fix
useless on its own.** All fifteen `NROS_LIFECYCLE_{STATE,TRANSITION,RET}_*`
constants were being **silently dropped** from `nros_generated.h` — each written
as `LifecycleState::X as u8`, a cast through an enum cbindgen cannot see.
Measured: 1 mention at the old HEAD, 19 after. So the signatures would have
compiled and C would still have had no name for a transition id or a callback's
return value. They are now literals guarded by a `const _` assertion block
against the Rust discriminants (note `Unconfigured == 1`, not 0). The rest of
`nros-c` was swept for the pattern; these were the only instances.

Verified: a TU calling all twelve entry points and every constant compiles clean
under `clang -std=c11 -Wall -Wextra -Werror`, `gcc -std=c11 -Werror` and
`clang++ -std=c++17 -Werror`.

### Fix shape (as originally proposed)

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

## Problem 2 — `nros_lifecycle_init` accepts a node and throws it away (FIXED 2026-08-25)

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

**Fixed by REMOVING the parameter, and `nros_make_node_a_lifecycle_node` with
it.** How the executor family reaches its node is what decided it: it does not
go through an `nros_node_t` at all. `Executor::register_lifecycle_services`
builds the five servers on the *executor's* session, stores them in the
executor's `LifecycleRuntimeState`, and relies on the executor's spin loop to
poll them. None of that is reachable from a caller-held state machine, and a
node pointer cannot confer it — so storing the node would have replaced a
visible lie with a quiet one. Every in-tree caller was a unit test fabricating
`&1u8 as *const nros_node_t`, which is its own evidence the parameter was
already meaningless.

**This is a deliberate step away from rclc parity, and it should be read as
one:** rclc has `rclc_make_node_a_lifecycle_node` and we now do not. The ledger
records it `declined`. Making the standalone family genuinely work is a
redesign — the handle would have to become a *view* onto the executor's machine
— not a bug fix.

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
