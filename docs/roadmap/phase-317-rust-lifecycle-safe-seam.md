# Phase 317 — Rust lifecycle safe seam (`LifecycleCallbacks` trait)

**Status (2026-07-28): planned.** Implements issue
[0335](../issues/archived/0335-examples-carry-framework-gaps.md) defect 2 — give
the Rust lifecycle surface a safe registration seam symmetric with the C++
`nros::LifecycleNode` already shipped in phase-270. RFC-0019 (thin C/C++/Rust
shim over the core), REP-2002 (managed nodes).

## The asymmetry

C++ is already rclcpp-shaped. A user subclasses `nros::LifecycleNode`
(`nros-cpp/include/nros/lifecycle.hpp`) and overrides safe virtuals:

```cpp
class MyNode : public nros::LifecycleNode {
    CallbackReturn on_configure(LifecycleState previous) override { /* … */ return CallbackReturn::Success; }
};
node.register_services();
```

Rust has **no** matching wrapper. The only surface is the raw C-FFI state
machine `LifecyclePollingNodeCtx::register(slot, Option<unsafe extern "C"
fn(*mut c_void) -> u8>)`, so `examples/native/rust/lifecycle-node` is forced to
hand-write five `unsafe extern "C"` callbacks returning a raw enum discriminant —
a shape no rclcpp/rclrs user would recognise (its own module doc admits it exists
"so this path exercises exactly the same FFI surface the C API uses", i.e. it is
an FFI regression test wearing an example's clothes).

## Design (approved 2026-07-28)

A **trait**, symmetric with the C++ virtual-override shape and rclrs-recognizable:

```rust
pub trait LifecycleCallbacks {
    fn on_configure(&mut self) -> TransitionResult { TransitionResult::Success }
    fn on_activate(&mut self) -> TransitionResult { TransitionResult::Success }
    fn on_deactivate(&mut self) -> TransitionResult { TransitionResult::Success }
    fn on_cleanup(&mut self) -> TransitionResult { TransitionResult::Success }
    fn on_shutdown(&mut self) -> TransitionResult { TransitionResult::Success }
    fn on_error(&mut self) -> TransitionResult { TransitionResult::Failure }
}

executor.register_lifecycle_node(&mut my_node)?; // binds the 5 REP-2002 services + trampolines
```

Symmetry: trait methods ↔ virtual overrides; defaulted Success…/Failure ↔ the
C++ non-pure-virtual defaults; `&mut self` ↔ `this`.

**No `previous` argument** (unlike rclcpp's `on_*(const State& previous)`): the
nano-ros FFI callback boundary (`LifecycleCallbackFnCtx = extern "C" fn(ctx) ->
u8`) carries only the user context, not the transition state — the C++ side
recovers `previous` because its trampoline holds `this`, which holds the executor
handle, but the Rust ctx is a bare `&mut T`. The existing safe
`LifecyclePollingNode` fn-pointer API is likewise state-less. A node that needs
the current state reads `Executor::lifecycle_state_machine().state()`. (Threading
state through would need a wrapper ctx carrying the executor — a later refinement
if a consumer needs it.)

**Alloc-free seam — monomorphized generic trampolines, no `Box`.**
`register_lifecycle_node::<T: LifecycleCallbacks>` registers, per slot, a generic
`extern "C" fn tramp_configure<T>(ctx: *mut c_void) -> u8` on the existing
`LifecyclePollingNodeCtx` (`register(slot, cb)` + `set_context(&mut node as *mut
c_void)`). rustc monomorphizes one `extern "C"` per `T`, so the seam works
`no_std`, matching the C++ side's allocation-free posture.

**Safety invariant:** the node must outlive the registration; the trampoline
reconstitutes `&mut T` only during a callback, and `spin` is single-threaded per
executor, so there is no concurrent `&mut` aliasing — the same guarantee C++
relies on for `this`.

## Work items

### W1 — the trait + registration seam

- **Add** `LifecycleCallbacks` (6 defaulted methods) to `nros-node`'s
  `lifecycle` module, re-exported from `nros::lifecycle`.
- **Add** `Executor::register_lifecycle_node<T: LifecycleCallbacks>(&mut self,
  node: &mut T) -> Result<(), NodeError>` — calls the existing
  `register_lifecycle_services()`, then per slot registers the monomorphized
  `tramp_*::<T>` and `set_context(node as *mut _ as *mut c_void)`.
- **Keep** `LifecyclePollingNodeCtx::register(slot, Option<extern fn>)` as the
  C-parity low-level seam the trait wraps — do not remove it.
- **Acceptance:** a unit test registers a trait impl whose `on_configure`
  toggles a field, drives a configure transition through the state machine, and
  asserts the field flipped and the reported state advanced — no `unsafe` in the
  test's node.

### W2 — example rewrite + FFI test relocation

- **Rewrite** `examples/native/rust/lifecycle-node/src/main.rs` to the trait:
  a `struct` with `impl LifecycleCallbacks`, `executor.register_lifecycle_node`,
  then spin. Zero `unsafe extern "C"` in the example.
- **Move** the raw-`extern "C" fn(ctx)` exercise (the current example body — an
  FFI regression test) into `packages/testing/nros-tests/`, asserting the
  low-level `LifecyclePollingNodeCtx` seam directly.
- **Acceptance:** `examples/native/rust/lifecycle-node` builds + runs (drive it
  with `ros2 lifecycle set`), contains no `extern "C"`; the relocated FFI test
  passes in `nros-tests`; `just ci` green.

## Non-goals

- No change to the C++ `LifecycleNode` (already rclcpp-shaped).
- No change to the CFFI ABI or the core state-machine logic — this is a
  type-adapting wrapper only (RFC-0019 C1: the shim carries no behavior).
