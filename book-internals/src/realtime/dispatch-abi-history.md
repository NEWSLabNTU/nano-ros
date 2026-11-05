# Dispatch ABI history

> The user-facing description of `DispatchStrategy`, the tag API and the
> `__nros_node_<pkg>_dispatch_strategy()` symbol is
> [Dispatch Strategy](../../realtime/dispatch-strategy.html) in the nano-ros
> Book. This page keeps the implementation history behind it: which phase
> built which piece, and which slots are still open.

The locked spec is
[`docs/roadmap/archived/phase-216-baremetal-framework-integration.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/roadmap/archived/phase-216-baremetal-framework-integration.md).

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
  the per-Node `#[isr_safe]` contract described on the user page. The
  Phase 214.J `atomic_waker` substrate is the building block for the SPSC
  variant.
- **Per-callback strategies (Phase 216.E.3).** The slot to reconsider
  per-Node granularity if a real Node needs mixed strategies.
- **Foreign host loops.** A drain loop owned by a host nano-ros does not
  generate is out of scope under
  [RFC-0113](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0113-rt-unified-wait.md).
