# Dispatch ABI history

> The user-facing description of `DispatchStrategy` and the tag API is the
> "Deferred dispatch" section of
> [The execution model](../../background/execution-model.html#deferred-dispatch)
> in the nano-ros Book. This page keeps what a contributor needs behind it:
> the ABI symbol, the board-side trait, the compatibility contract, which
> phase built which piece, and which slots are still open.

The locked spec is
[`docs/roadmap/archived/phase-216-baremetal-framework-integration.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/roadmap/archived/phase-216-baremetal-framework-integration.md).

## The `__nros_node_<pkg>_dispatch_strategy()` symbol

`nros::node!()` emits one symbol per node package:

```rust
// Emitted by nros::node!(Talker).
#[unsafe(no_mangle)]
extern "C" fn __nros_node_talker_pkg_dispatch_strategy() -> u8 {
    <Talker as ::nros::Node>::DISPATCH as u8
}
```

Its consumers:

1. **`nros check`** reads it from the node crate's `.rmeta` (or, on the host,
   links the staticlib and reads it through `dlsym`) and compares it with the
   entry board's `framework` using the framework-by-strategy matrix:
   `Inline` and `Deferred` are accepted on posix, rtos and bare-metal;
   `FromIsr` is refused on all three.
2. **`nros::main!()`** walks the registered nodes and reads each strategy. An
   entry with a dispatch task adds it only when some node is `Deferred`; the
   entries generated for the supported boards have none and register every
   node inline.
3. **Introspection tools** can read the symbols from a linked binary and print
   the (package, strategy) table without the source.

`#[repr(u8)]` with `Inline = 0`, `Deferred = 1`, `FromIsr = 2` is
wire-frozen. A new strategy is a new discriminant, never a renumbering;
`FromIsr` was reserved up front for exactly that reason, and so the `nros
check` matrix can match on the enum instead of on a spelling.

## The board-side trait

`NodeDispatchRuntime` (in `packages/platform/nros-platform/src/board/runtime.rs`)
carries `signal_callback` and `dispatch_strategy`, both defaulted:
`dispatch_strategy` returns `Inline`, and `signal_callback` panics. The
Inline path never calls `signal_callback`, so the panic is louder than a
dropped callback if a hand-written entry signals an Inline runtime. A
Deferred runtime overrides both: `signal_callback` enqueues `(cb_id,
snapshot)` on a single-producer queue (only the spin task enqueues) and wakes
the dispatch task.

## Compatibility contract

- `Node::DISPATCH` is a defaulted associated const (`Inline`), so every
  `impl Component` that does not mention it compiles unchanged.
- The closure registration path (`create_subscription`, `create_service`,
  `create_action`) stays valid for Inline nodes. The macro lint rejects a
  closure only when `DISPATCH = Deferred`, and points at the `_static` form.
- The tag's `PartialEq<Callback<'_>>` compares `&'static str`s, a pointer
  compare in the common case where the runtime hands back the registered
  `&'static`.

## What `FromIsr` needs

- **A reentrancy audit** of the dispatch path (`signal_callback`, queue push,
  RMW raw-CDR buffer ownership) against ISR-priority producers. Today's path
  assumes thread-context callers.
- **A lock-free SPSC variant.** `heapless::spsc::Queue` is single-producer,
  single-consumer at thread priority; an ISR producer with a thread consumer
  needs at least a barrier on the ISR-side push.
- **A per-node `#[isr_safe]` contract:** `on_callback` must not block, panic
  or allocate.

## Why per-node, not per-callback

Per-callback strategies would multiply the framework-by-strategy matrix by
the callbacks per node, and fork the dispatch path (each queue entry tagged
with its own strategy at enqueue time). The dispatch task drains one queue
and routes each entry by `CallbackId` to one node's `on_callback`. No user
has needed mixed strategies; the slot to reconsider is below.

## Where each piece came from

| Piece | Work item |
|---|---|
| `DispatchStrategy` enum, Inline default for every Node | Phase 216 |
| Tag types (`SubscriptionTag`, `ServiceTag`, `ActionTag`) and the `_static` registration variants | Phase 216.A.4 |
| `__nros_node_<pkg>_dispatch_strategy()` emitted by `nros::node!()` | Phase 216.A.5 |
| Macro lint rejecting closure registration on a Deferred Node | Phase 216.A.6 |
| `nros::main!()` emitting the `__nros_dispatch` task only when a Node is Deferred | Phase 216.B.3 / C.3 |
| Framework × strategy matrix in `nros check` | Phase 216.D.1 |
| `atomic_waker` cross-task notification substrate | Phase 214.J |
| Board-side sink renamed `NodeRuntime` → `NodeDispatchRuntime` | Phase 214.K.1 |
| Executor-level deferral without the macros (`set_deferred`, `set_defer_notifier`, `run_deferred`, `run_deferred_handle`) | issue 2146 |

Before Phase 216 every Node was Inline-by-default (Phase 212); the
defaulted `Node::DISPATCH` const exists so those Node packages keep
compiling unchanged.

## The trait surface split

Phase 214.K.1 renamed the board-side dispatch sink from `NodeRuntime` to
`NodeDispatchRuntime`; the user-facing sink kept the `NodeRuntime` name and
now lives in `packages/api/nros/src/node.rs`. Phase 216 added
`signal_callback` and `dispatch_strategy` to `NodeDispatchRuntime`, both
defaulted so the existing Inline implementations (`ExecutorNodeRuntime` in
`nros`, `NullNodeRuntime` in `nros-platform`) needed no change.

## Open slots

- **`FromIsr` (Phase 216.E.1).** The discriminant is reserved; the
  implementation needs the reentrancy audit, the lock-free SPSC variant and
  the per-Node `#[isr_safe]` contract above. The
  Phase 214.J `atomic_waker` substrate is the building block for the SPSC
  variant.
- **Per-callback strategies (Phase 216.E.3).** The slot to reconsider
  per-Node granularity if a real Node needs mixed strategies.
- **Foreign host loops.** A drain loop owned by a host nano-ros does not
  generate is out of scope under
  [RFC-0113](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0113-rt-unified-wait.md).
