# Dispatch Strategy

> Phase 216 design rationale. This chapter explains *why*
> `DispatchStrategy` is shaped the way it is — the trichotomy, the
> per-Node granularity, the tag-based callback API, the
> `__nros_node_<pkg>_dispatch_strategy()` ABI symbol, and the
> backward-compat contract. For the user-facing tutorials see
> [RTIC Integration](../user-guide/rtic-integration.md) and
> [Embassy Integration](../user-guide/embassy-integration.md).

## The Inline / Deferred / FromIsr trichotomy

A nano-ros Node declares `Node::DISPATCH: DispatchStrategy` to tell the
codegen + lint layers how its callbacks need to be delivered:

```rust
// File: packages/core/nros-platform/src/board/dispatch.rs
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
  (preserves every pre-Phase-216 Node pkg unchanged). Served by every
  runtime: POSIX, FreeRTOS, NuttX, Zephyr, ThreadX, bare-metal, RTIC
  (proxied via `__nros_dispatch` task when needed), Embassy
  (likewise — though Inline is unusual under cooperative async; see
  the matrix below).

- **`Deferred`** — Callbacks land in a board-side queue
  (`heapless::spsc::Queue` on RTIC, `embassy_sync::channel::Channel`
  on Embassy). A framework-owned dispatch task drains the queue and
  drives `ExecutableNode::on_callback` from its own task context,
  decoupling callback latency from network polling. Required for
  callback-driven Nodes on RTIC + Embassy.

- **`FromIsr`** — Callbacks fire directly from an ISR handler. **Design
  slot only** — implementation deferred to Phase 216.E.1. Reserved so
  the lint matrix has a stable discriminant to reject against;
  current builds error out at the `nros check` layer.

### Framework × Strategy matrix

The matrix `nros check` (Phase 216.D.1) enforces:

```
              ┌──────────────────────────────────────────────────────┐
              │            DispatchStrategy                          │
              │  Inline      │  Deferred       │  FromIsr            │
┌─────────────┼──────────────┼─────────────────┼─────────────────────┤
│ posix       │  OK          │  OK             │  ERR: no ISRs       │
│ rtos        │  OK          │  OK             │  ERR: no ISRs       │
│ rtic        │  WARN: pref. │  OK (canonical) │  FUTURE (216.E.1)   │
│             │  Deferred    │                 │                     │
│ embassy     │  WARN: pref. │  OK (canonical) │  ERR: no ISR exec   │
│             │  Deferred    │                 │                     │
└─────────────┴──────────────┴─────────────────┴─────────────────────┘
```

- **POSIX + RTOS**: both Inline and Deferred work. Inline is the
  default because the executor's spin loop *is* the natural place to
  dispatch callbacks on hosted targets.
- **RTIC + Embassy**: Deferred is canonical. Inline is permitted (the
  framework adapter still serves it) but `nros check` warns:
  callback-driven Nodes that run inline tie their handler latency to
  spin task scheduling, which is rarely what you want under
  hardware-interrupt or async-cooperative scheduling.
- **`FromIsr` on POSIX/RTOS**: rejected — there's no meaningful "ISR
  context" for nano-ros to dispatch from on a hosted OS.
- **`FromIsr` on RTIC**: future — needs the reentrancy audit + SPSC
  rework + per-Node `#[isr_safe]` contract called out in Phase
  216.E.1.
- **`FromIsr` on Embassy**: rejected — Embassy has no concept of
  "callback fires from an ISR handler"; ISR-driven work hands off to
  an async task via `embassy_sync::signal::Signal` or similar.

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
high-priority lock while another doesn't — Phase 216.E.3 is the slot
to reconsider. Until then: YAGNI.

## Why FromIsr is a design slot, not an impl

The `FromIsr` discriminant exists in the enum today so that:

- The `[repr(u8)]` discriminants are stable. `Inline = 0`,
  `Deferred = 1`, `FromIsr = 2` are wire-frozen — the
  `__nros_node_<pkg>_dispatch_strategy()` ABI symbol returns a `u8`
  that `nros check` reads without linking the Node crate. Adding
  `FromIsr` later (when 216.E.1 lands) would either renumber existing
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

The substrate Phase 214.J built (`atomic_waker` for cross-task
notification) is the building block for the SPSC variant; landing
214.J was a precondition for being able to even *prototype*
`FromIsr`. The full impl is deferred until a real ISR-driven
driver demands it.

## Tag-based callback API rationale

The closure-based registration API:

```rust
// Pre-216.A.4 — still valid for Inline Nodes.
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

Both options conflict with the no-alloc + framework-task-routed
contract. So Phase 216.A.4 introduces **tags**:

```rust
// File: packages/core/nros/src/dispatch_tag.rs
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
macro lint (216.A.6): a Deferred Node using `create_subscription` (the
closure form) fails to compile with a clear error pointing at the
registration call and suggesting the `_static` form. An Inline Node
using `_static` is allowed — but Deferred → closure is rejected.
Zero migration cost for pre-216 Node pkgs (all defaulted to Inline)
and a forced-correct API for the Deferred path.

## The `__nros_node_<pkg>_dispatch_strategy()` ABI symbol

The `nros::node!()` macro emits, per Node pkg:

```rust
// Emitted by nros::node!(Talker) — Phase 216.A.5.
#[unsafe(no_mangle)]
extern "C" fn __nros_node_talker_pkg_dispatch_strategy() -> u8 {
    <Talker as ::nros::Node>::DISPATCH as u8
}
```

Three consumers care about this symbol:

1. **`nros check` (Phase 216.D.1).** Statically inspects the Node
   crate's `.rmeta` or links the staticlib + reads the symbol via
   `dlsym`/`GetProcAddress` (host-side check; embedded targets only
   read it from `.rmeta`). Compares against the Entry pkg's board
   `framework` metadata using the matrix above; rejects mismatches at
   `nros check` time, *before* the user runs `cargo build`.

2. **The `nros::main!()` proc-macro (Phase 216.B.3 / C.3).** When
   expanding the Entry pkg's `main.rs` it walks the registered Node
   list and reads each pkg's strategy. If any Node is Deferred, the
   generated `#[rtic::app]` / `#[embassy_executor::main]` body
   includes the `__nros_dispatch` task; if all are Inline, the
   dispatch task is omitted (zero overhead for Inline-only
   workspaces).

3. **Future runtime diagnostic tools.** A `nros doctor` or `nros
   topology` style introspection tool can read the symbols from a
   linked binary and print the (pkg, strategy) table without
   having to re-parse the source. Useful for post-mortem on a
   binary you didn't build yourself.

The `extern "C"` + `[repr(u8)]` ABI is the contract. It must not
break across nano-ros versions — adding a new strategy variant means
adding a new discriminant (`FromIsr = 2` was reserved up front
exactly to avoid this), never renumbering an existing one.

## The trait surface split (post-214.K.1)

Phase 214.K.1 renamed the board-side dispatch sink from `NodeRuntime`
to `NodeDispatchRuntime` (the user-facing sink kept the
`NodeRuntime` name, which is now in `packages/core/nros/src/node.rs`).
Phase 216 lands its new methods on `NodeDispatchRuntime`:

```rust
// File: packages/core/nros-platform/src/board/runtime.rs
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
// File: packages/boards/nros-board-rtic-stm32f4/src/runtime.rs (sketch)
impl NodeDispatchRuntime for RticRuntime {
    fn dispatch_strategy(&self) -> DispatchStrategy {
        DispatchStrategy::Deferred
    }
    fn signal_callback(&mut self, cb_id: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
        // SAFETY: SPSC producer is single-threaded by RTIC priority assignment.
        self.queue_producer.enqueue((cb_id, ctx.snapshot())).unwrap();
        // Wake the dispatch task; RTIC will schedule it at its declared priority.
        __nros_dispatch::spawn().ok();
    }
}
```

`signal_callback`'s default panic is the right behavior: the Inline
path never calls it (callbacks flow through the existing inline
trampoline). If a Deferred Node ends up on an Inline runtime — for
example because the user manually picked the wrong board — the panic
is louder than silently dropping the callback. The lint at
Phase 216.D.1 prevents this combination from compiling in the first
place; the panic is a belt-and-braces backstop.

## Backward compatibility

Two contracts:

1. **Defaulted associated const.** `Node::DISPATCH` is
   `const DISPATCH: DispatchStrategy = DispatchStrategy::Inline;` in
   the trait definition. Edition 2024 supports defaulted associated
   consts as stable, so every pre-216 `impl Node for ...` block that
   doesn't mention `DISPATCH` continues to compile and is treated as
   `Inline`.
2. **Closure API preserved on the Inline path.** The Inline runtime
   keeps the closure-based registration path
   (`create_subscription`, `create_service`, `create_action`). The
   macro lint only rejects closure use when `DISPATCH = Deferred`.
   Every Phase 212 Node pkg using closures stays valid without
   changes.

The migration shape for a Phase 212 Node that wants to move to
Deferred is:

```rust
// Before — Phase 212 Inline-by-default.
impl Node for Listener {
    const NAME: &'static str = "listener";
    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        ctx.create_subscription::<Int32>("/chatter", |msg| {
            defmt::info!("Received: {}", msg.data);
        })?;
        Ok(())
    }
}

// After — Phase 216 Deferred.
impl Node for Listener {
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

- [RTIC Integration](../user-guide/rtic-integration.md) — user-facing
  tutorial for the RTIC side of dispatch.
- [Embassy Integration](../user-guide/embassy-integration.md) —
  user-facing tutorial for the Embassy side of dispatch +
  spawn-from-sync.
- [Scheduling Models](./scheduling-models.md) — the real-time
  scheduling backdrop against which dispatch strategy choices are
  made.
- `docs/roadmap/phase-216-baremetal-framework-integration.md` — the
  locked spec.
- `packages/core/nros-platform/src/board/dispatch.rs` — the
  `DispatchStrategy` enum.
- `packages/core/nros/src/dispatch_tag.rs` — the tag types.
