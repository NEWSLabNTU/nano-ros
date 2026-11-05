# Dispatch Strategy

<!-- type: explanation · level: 3 -->

> Design rationale. This chapter explains *why*
> `DispatchStrategy` is shaped the way it is — the trichotomy, the
> per-Node granularity, the tag-based callback API, the
> `__nros_node_<pkg>_dispatch_strategy()` ABI symbol, and the
> backward-compat contract. Embassy and other foreign async runtimes are not
> supported ([RTOS Cooperation](rtos-cooperation.md) says why).

## The Inline / Deferred / FromIsr trichotomy

A nano-ros Node declares `Node::DISPATCH: DispatchStrategy` to tell the
codegen + lint layers how its callbacks need to be delivered:

```rust
// File: packages/platform/nros-platform/src/board/dispatch.rs
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum DispatchStrategy {
    Inline = 0,
    Deferred = 1,
    FromIsr = 2,
}
```

The variants:

- **`Inline`** — Callbacks fire from the executor's spin loop, in the
  same task that drives transport I/O. Default for every Node
  (a Node that does not name a strategy keeps working unchanged). Served by every
  runtime: POSIX, FreeRTOS, NuttX, Zephyr, ThreadX and bare-metal.

- **`Deferred`** — Callbacks land in a queue, and a dispatch task drains it
  and drives `ExecutableNode::on_callback` from its own task context,
  decoupling callback latency from network polling. The entry `nros::main!`
  generates for a supported board has no such task: each tier's loop is the
  only task that runs its callbacks, so a `Deferred` component is registered
  inline there (a queue nothing drains would lose callbacks). The node still
  has to use the tag API below, so it builds unchanged for an entry that does
  drain the queue.

- **`FromIsr`** — Callbacks fire directly from an ISR handler. **Design
  slot only** — not implemented yet. Reserved so
  the lint matrix has a stable discriminant to reject against;
  current builds error out at the `nros check` layer.

### Framework × Strategy matrix

The matrix `nros check` enforces:

```
              ┌──────────────────────────────────────────────────────┐
              │            DispatchStrategy                          │
              │  Inline      │  Deferred       │  FromIsr            │
┌─────────────┼──────────────┼─────────────────┼─────────────────────┤
│ posix       │  OK          │  OK             │  ERR: no ISRs       │
│ rtos        │  OK          │  OK             │  ERR: no ISRs       │
│ bare-metal  │  OK          │  OK             │  ERR: no ISRs       │
└─────────────┴──────────────┴─────────────────┴─────────────────────┘
```

- **Inline and Deferred**: both accepted on every supported board. Inline is
  the default because the executor's spin loop *is* the natural place to
  dispatch callbacks when the entry owns that loop.
- **`FromIsr`**: rejected — the entries nano-ros generates dispatch from a
  task or loop, never from an ISR handler, so there is no ISR context to
  dispatch from. It stays rejected until the reentrancy audit, the SPSC
  rework and the per-Node `#[isr_safe]` contract described below exist.

## Why per-Node, not per-callback

A Node that wants its `/chatter` subscription to run inline but its
`/heartbeat` subscription to run deferred is conceptually expressible —
we could put `DispatchStrategy` on each `create_subscription` call.
We don't, for two reasons:

1. **The lint matrix collapses.** Per-callback strategies multiply the
   `(framework, strategy)` matrix by the number of callbacks per
   Node. The error surface grows quadratically and the messages get
   harder to phrase. Per-Node keeps each Node either fully Inline or
   fully Deferred — one strategy to reason about per pkg.
2. **Implementation simplicity.** The dispatch task drains a single
   queue and routes each entry by `CallbackId` to a single Node's
   `on_callback`. Per-callback would mean tagging each entry with its
   own strategy at enqueue time and forking the dispatch path. Adds
   weight for a feature no real user has asked for yet.

If a real user demonstrates a Node that genuinely wants mixed
strategies — typically because one subscription handler must hold a
high-priority lock while another doesn't — per-callback strategies can be
reconsidered. Until then: YAGNI.

## Why FromIsr is a design slot, not an impl

The `FromIsr` discriminant exists in the enum today so that:

- The `[repr(u8)]` discriminants are stable. `Inline = 0`,
  `Deferred = 1`, `FromIsr = 2` are wire-frozen — the
  `__nros_node_<pkg>_dispatch_strategy()` ABI symbol returns a `u8`
  that `nros check` reads without linking the Node crate. Adding
  `FromIsr` later would either renumber existing
  discriminants (breaking already-compiled Node binaries) or require
  a new ABI symbol.
- The lint matrix has somewhere to point. Without the variant in the
  enum, the `nros check` matrix would have to special-case "user
  wrote `FromIsr` but it doesn't exist yet" via spelling-comparison
  rather than enum match. Worse error messages, worse evolvability.

The actual implementation needs three pieces that aren't there yet:

- **Reentrancy audit.** Every step of the dispatch path —
  `signal_callback`, queue push, RMW raw-CDR buffer ownership — must
  be re-entrant against ISR-priority producers. Today's path assumes
  thread-context callers.
- **Lock-free SPSC variant.** `heapless::spsc::Queue` is single-
  producer / single-consumer at thread priority. ISR-priority
  producer + thread-priority consumer needs a stronger ordering
  contract (memory barrier on the ISR-side push at minimum, possibly
  a different queue type).
- **Per-Node `#[isr_safe]` proof contract.** `on_callback` must not
  call anything that can block, panic, or allocate. Statically
  proving that for arbitrary user code is a documentation +
  attribute exercise we haven't undertaken.

The executor's `atomic_waker` cross-task notification is the building
block for the SPSC variant. The full impl is deferred until a real ISR-driven
driver demands it.

## Tag-based callback API rationale

The closure-based registration API:

```rust
// Closure form — valid for Inline Nodes.
ctx.create_subscription("/chatter", |msg: Int32| {
    handle(msg);
});
```

works fine when the callback runs synchronously on the spin task: the
closure captures state by value (or by `&mut`) and gets called inline
during `spin_once`. The captured environment lives on the spin
task's stack frame, no heap, no boxing.

Deferred dispatch breaks that assumption. The callback now fires from
*a different task* than the one that called `register`, which means
the closure environment must either:

- Outlive both tasks (require `'static` capture — `move ||`
  everywhere, but state still needs somewhere to live), or
- Be stored generically in the runtime, which means erasing the
  closure type to `Box<dyn FnMut(...)>` (alloc-dependent) or to
  `extern "C" fn` (no captures at all).

Both options conflict with keeping dispatch heap-free and
framework-task-routed. (Heap-free here is a checked property of the
executor path, not of an image with a transport: `heap-free-poc-mps2`
links the executor, component install and spin path with no allocation
symbol, and the L3 CI lane fails if that changes. Every shipped RMW backend
allocates in C.) So deferred dispatch uses **tags**:

```rust
// File: packages/api/nros/src/dispatch_tag.rs
pub struct SubscriptionTag(&'static str);
pub struct ServiceTag(&'static str);
pub struct ActionTag(&'static str);

impl From<SubscriptionTag> for CallbackId<'static> { /* ... */ }
impl PartialEq<Callback<'_>> for SubscriptionTag { /* ... */ }
```

The tag carries only the `&'static str` callback identifier — zero
runtime cost, no captures, FFI-safe. State lives on
`ExecutableNode::State`; the macro-emitted `init()` body wires the
tag fields by calling the `_static` registration variants
(`create_subscription_static` / `create_service_static` /
`create_action_static`) and storing the returned tag onto
`Self::State`.

Dispatch then matches the tag against the `Callback<'_>` event:

```rust
fn on_callback(state: &mut Self::State, cb: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
    if state.sub_chatter == cb {
        let msg: Int32 = ctx.downcast().unwrap();
        // ...
    } else if state.sub_heartbeat == cb {
        let beat: Heartbeat = ctx.downcast().unwrap();
        // ...
    }
}
```

The `PartialEq<Callback<'_>>` impl on each tag type does the
comparison in `&'static str` terms — `O(ptr_eq)` in the common case
when the runtime hands back the same `&'static` the user registered.

**Inline keeps closures.** The closure-vs-tag split is enforced by the
macro lint: a Deferred Node using `create_subscription` (the
closure form) fails to compile with a clear error pointing at the
registration call and suggesting the `_static` form. An Inline Node
using `_static` is allowed — but Deferred → closure is rejected.
Zero migration cost for existing Node pkgs (all default to Inline)
and a forced-correct API for the Deferred path.

## Deferring callbacks without the macros

`Node::DISPATCH` belongs to `nros::node!` nodes. The queue under it is the
executor's own, with three calls and no macro-private type. A loop that drains
it must be a task whose priority nano-ros owns, such as a tier's; a foreign
host's loop would decide when the callbacks run, which nano-ros does not
support:

```rust
use nros::{Executor, HandleId};

// 1. Mark the callbacks the host wants to run in its own task(s).
let sub: HandleId = node.create_subscription::<Int32, _>("/chatter", on_chatter)?;
executor.set_deferred(sub, true);

// 2. Learn when one is queued. A plain fn and a context pointer; it runs on
//    the spinning thread, inside the pass, and must not block.
fn on_queued(ctx: *mut core::ffi::c_void, handle: HandleId) {
    let _ = (ctx, handle); // pend / wake the task that owns `handle`
}
executor.set_defer_notifier(on_queued, core::ptr::null_mut());

// 3. In the spin task: spin as usual. A ready deferred callback is QUEUED
//    and notified, never run here.
executor.spin_once(core::time::Duration::from_millis(10));

// In the host's task: run what is queued, all of it or one callback.
executor.run_deferred();            // every queued callback, slot order
executor.run_deferred_handle(sub);  // just this one (per-priority tasks)
```

The rules:

- The queue is one bit per callback slot, so it allocates nothing. A callback
  that is already queued is not queued or notified again; a drain runs it once,
  and that dispatch takes what an inline dispatch would have. Data left behind
  is found by the next spin, which queues it again.
- A deferred callback takes no part in the spin's `Trigger`.
- Timers cannot be deferred (`set_deferred` returns `false`): a timer's
  readiness is decided inside its own dispatch.
- The defer notifier fires during the spin and says which callback to run.
- The drain needs `&mut Executor`, like `spin_once`. On a single task that is
  the same task; across tasks, the executor is the shared resource already
  guarded for spinning.

## The `__nros_node_<pkg>_dispatch_strategy()` ABI symbol

The `nros::node!()` macro emits, per Node pkg:

```rust
// Emitted by nros::node!(Talker).
#[unsafe(no_mangle)]
extern "C" fn __nros_node_talker_pkg_dispatch_strategy() -> u8 {
    <Talker as ::nros::Node>::DISPATCH as u8
}
```

Three consumers care about this symbol:

1. **`nros check`.** Statically inspects the Node
   crate's `.rmeta` or links the staticlib + reads the symbol via
   `dlsym`/`GetProcAddress` (host-side check; embedded targets only
   read it from `.rmeta`). Compares against the Entry pkg's board
   `framework` metadata using the matrix above; rejects mismatches at
   `nros check` time, *before* the user runs `cargo build`.

2. **The `nros::main!()` proc-macro.** When
   expanding the Entry pkg's `main.rs` it walks the registered Node
   list and reads each pkg's strategy. An entry with a dispatch task adds
   it only when some Node is Deferred, so Inline-only workspaces pay
   nothing for it; the entries generated for the supported boards have
   none and register every Node inline.

3. **Future runtime diagnostic tools.** A `nros doctor` or `nros
   topology` style introspection tool can read the symbols from a
   linked binary and print the (pkg, strategy) table without
   having to re-parse the source. Useful for post-mortem on a
   binary you didn't build yourself.

The `extern "C"` + `[repr(u8)]` ABI is the contract. It must not
break across nano-ros versions — adding a new strategy variant means
adding a new discriminant (`FromIsr = 2` was reserved up front
exactly to avoid this), never renumbering an existing one.

## The board-side dispatch trait

The board-side dispatch sink is `NodeDispatchRuntime` (the user-facing
sink is `NodeRuntime`, in `packages/api/nros/src/node.rs`). It carries
the two dispatch methods:

```rust
// File: packages/platform/nros-platform/src/board/runtime.rs
pub trait NodeDispatchRuntime {
    // ... existing methods unchanged ...

    fn signal_callback(&mut self, _cb_id: CallbackId<'_>, _ctx: &mut CallbackCtx<'_>) {
        panic!("signal_callback not implemented for Inline runtime");
    }
    fn dispatch_strategy(&self) -> DispatchStrategy {
        DispatchStrategy::Inline
    }
}
```

Both methods are defaulted — zero-touch for the existing `Inline`
impls (`ExecutorNodeRuntime` in `nros`, `NullNodeRuntime` in
`nros-platform`). A Deferred runtime overrides both:

```rust
// A sketch: a board runtime with a dispatch task of its own.
impl NodeDispatchRuntime for DeferredRuntime {
    fn dispatch_strategy(&self) -> DispatchStrategy {
        DispatchStrategy::Deferred
    }
    fn signal_callback(&mut self, cb_id: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
        // Single producer: only the spin task enqueues.
        self.queue_producer.enqueue((cb_id, ctx.snapshot())).unwrap();
        // Wake the dispatch task; it runs at its own declared priority.
        self.wake_dispatch_task();
    }
}
```

`signal_callback`'s default panic is the right behavior: the Inline
path never calls it (callbacks flow through the existing inline
trampoline). If a callback is signalled to an Inline runtime anyway —
for example from a hand-written entry — the panic is louder than
silently dropping the callback. A generated entry never reaches it:
`nros::main!` registers a Deferred Node inline when the runtime has no
dispatch task, so the panic is a belt-and-braces backstop.

## Backward compatibility

Two contracts:

1. **Defaulted associated const.** `Node::DISPATCH` is
   `const DISPATCH: DispatchStrategy = DispatchStrategy::Inline;` in
   the trait definition. Edition 2024 supports defaulted associated
   consts as stable, so every `impl Component for ...` block that
   doesn't mention `DISPATCH` continues to compile and is treated as
   `Inline`.
2. **Closure API preserved on the Inline path.** The Inline runtime
   keeps the closure-based registration path
   (`create_subscription`, `create_service`, `create_action`). The
   macro lint only rejects closure use when `DISPATCH = Deferred`.
   Every Node pkg using closures stays valid without changes.

The migration shape for an Inline Node that wants to move to
Deferred is:

```rust
// Before — Inline (the default).
impl Component for Listener {
    const NAME: &'static str = "listener";
    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        ctx.create_subscription::<Int32>("/chatter", |msg| {
            defmt::info!("Received: {}", msg.data);
        })?;
        Ok(())
    }
}

// After — Deferred.
impl Component for Listener {
    const NAME: &'static str = "listener";
    const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;
    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let _tag = ctx.create_subscription_static::<Int32>("/chatter")?;
        Ok(())
    }
}

impl ExecutableNode for Listener {
    type State = ListenerState;
    fn init() -> Self::State {
        ListenerState { sub_chatter: SubscriptionTag::placeholder() }
    }
    fn on_callback(state: &mut Self::State, cb: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if state.sub_chatter == cb {
            let msg: Int32 = ctx.downcast().unwrap();
            defmt::info!("Received: {}", msg.data);
        }
    }
}
```

The migration adds three things: the `DISPATCH` const, the tag-typed
`State` field, and the `on_callback` body. The closure body in the
"before" version becomes the `if state.sub_chatter == cb { ... }`
branch in the "after" version — same code, hoisted to a method.

## See also

- [RTOS Cooperation](rtos-cooperation.md) — the supported execution
  shapes, and why Embassy is not one.
- [Scheduling Models](../guide/realtime/pick-a-scheduling-model.md) — the real-time
  scheduling backdrop against which dispatch strategy choices are
  made.
- `packages/platform/nros-platform/src/board/dispatch.rs` — the
  `DispatchStrategy` enum.
- `packages/api/nros/src/dispatch_tag.rs` — the tag types.
- Changing this in the tree: see
  [Internals → Dispatch ABI history](../internals/realtime/dispatch-abi-history.html).

## Design background

- [RFC-0113: every supported way to run nano-ros keeps the real-time promise](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0113-rt-unified-wait.md)
