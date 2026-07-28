---
id: 335
title: "Two copy-out examples carry framework gaps: a PX4 weak-symbol link stub and five raw extern \"C\" lifecycle callbacks the user is expected to write"
status: open
type: bug
severity: medium
area: examples
related: [rfc-0026, issue-0050]
---

## Finding (audit 2026-07-28, P2)

All 67 `no_mangle` / `extern "C"` / `__nros_` hits under `examples/` were
triaged. The large majority are **legitimate** — 40+ are comments or Cargo.toml
notes *explaining* that the board crate or codegen owns a symbol
(`__nros_spin`, `__nros_entry_setup`, `nsh_main`, `rust_main`), the
`custom-transport-{talker,listener}` pair correctly moved its four callbacks into
`nros-transport-callbacks`, and the `native_entry/src/main.{c,cpp}` placeholders
are documented sentinels. Two are real framework gaps.

### 1. A weak-symbol link stub inside an example

`examples/px4/cpp/uorb/nros-register-check/sitl_register_stub.c:1` — the file's
own header says it: "SITL build scaffold (phase-244 D5) — NOT application logic".

It exists because the PX4-SITL build path does not link the `nros-rmw-cffi`
staticlib that ships the strong `nros_rmw_cffi_register`, so
`CMakeLists.txt:35-67` compiles a weak definition into the module. This is
exactly the RFC-0026 violation J1 describes — and note it was already *hoisted
out of* the example's own source, but only as far as a sibling file in the same
example directory.

Fix: move the weak definition into `nros-platform-px4` or the PX4 cmake module
(the weak-symbol gate machinery from archived issue 0050 / phase-247 already
exists) and delete the file from the example.

### 2. The Rust lifecycle example makes the user write C FFI

`examples/native/rust/lifecycle-node/src/main.rs:44` — the user writes five
`unsafe extern "C" fn(*mut c_void) -> u8` callbacks returning a raw enum
discriminant.

The module doc at :27-29 states the reason outright: "they are written as
`extern "C" fn` so this path exercises exactly the same FFI surface the C API
uses" — i.e. this is an **FFI regression test wearing an example's clothes**.
rclcpp and rclrs lifecycle users override safe methods or pass closures; no ROS 2
user would recognise this shape.

Fix: give `nros::lifecycle` a safe registration seam
(`sm.on_configure(|| TransitionResult::Success)` over `Fn`-to-`extern "C"`
trampolines — the shape `nros-transport-callbacks` already uses), and move the
raw-FFI exercise into `packages/testing/`.

## Related P3 (not filed)

`examples/**` still holds ~5.1 GB of orphaned build output in husks of examples
that were moved or deleted (`examples/native/rust/entry-poc` 3.4 G,
`examples/qemu-arm-baremetal/rust/phase216-rtic-e2e` 1.7 G, plus
`examples/zephyr/rust/{xrce,dds}`, `examples/qemu-esp32-baremetal/rust/dds`,
`examples/zephyr/rust/service-client-async`,
`examples/workspaces/mixed/src/c_add_client_pkg`). Their contents are fully
gitignored so `git status` is clean and nothing flags them. A `just clean`-style
sweep for example dirs with no tracked files would collect them. Recorded in
`docs/development/audit-findings-2026-07-28.md`.

## Progress

**Defect 1 — FIXED (b47f3d481).** The PX4-SITL weak `nros_rmw_cffi_register`
stub moved out of the example into the uORB backend
(`packages/px4/nros-rmw-uorb/src/register_fallback.c`) + weak-symbols allowlist.

## Design note — defect 2 (Rust lifecycle safe seam)

Approved shape (2026-07-28): a **trait**, symmetric with the shipped C++
`nros::LifecycleNode` (rclcpp `LifecycleNodeInterface`) and rclrs-recognizable.
C++ is already rclcpp-shaped (`nros-cpp/include/nros/lifecycle.hpp` — virtual
`on_*(LifecycleState previous) -> CallbackReturn` overrides + `this` trampolines).
The Rust side is the only gap; the example is forced into raw
`unsafe extern "C" fn(ctx) -> u8` because no safe wrapper exists.

Shape:

```rust
pub trait LifecycleCallbacks {
    fn on_configure(&mut self, previous: LifecycleState) -> TransitionResult { TransitionResult::Success }
    fn on_activate(&mut self, previous: LifecycleState) -> TransitionResult { TransitionResult::Success }
    fn on_deactivate(&mut self, previous: LifecycleState) -> TransitionResult { TransitionResult::Success }
    fn on_cleanup(&mut self, previous: LifecycleState) -> TransitionResult { TransitionResult::Success }
    fn on_shutdown(&mut self, previous: LifecycleState) -> TransitionResult { TransitionResult::Success }
    fn on_error(&mut self, previous: LifecycleState) -> TransitionResult { TransitionResult::Failure }
}

// binds the five REP-2002 services + the per-slot trampolines
executor.register_lifecycle_node(&mut my_node)?;
```

Symmetry with C++: trait methods ↔ virtual overrides; defaulted
Success/Success…/Failure ↔ the C++ non-pure-virtual defaults; `&mut self` ↔
`this`. `previous` = `get_state()` at callback entry (the SM invokes the callback
before committing the new state), exactly as the C++ `tramp_*` do.

Implementation seam (alloc-free, no `Box`): monomorphized generic trampolines.
`register_lifecycle_node::<T: LifecycleCallbacks>` registers, per slot, a generic
`extern "C" fn tramp_configure<T>(ctx: *mut c_void) -> u8 { let n = &mut *(ctx as
*mut T); n.on_configure(current_state()) as u8 }` on the existing
`LifecyclePollingNodeCtx` (`register(slot, cb)` + `set_context(&mut node as *mut
c_void)`). rustc monomorphizes one `extern "C"` per `T`, so no closure box is
needed — this works `no_std`, matching the C++ side's allocation-free posture.

Safety invariant to document: the node must outlive the registration, and the
trampoline reconstitutes `&mut T` only during a callback. `spin` is
single-threaded per executor, so no concurrent `&mut` aliasing — the same
guarantee C++ relies on for `this`.

Then: rewrite `examples/native/rust/lifecycle-node` to the trait shape, and move
the raw-`extern "C"`-fn exercise (the current example body, which is really an
FFI regression test) into `packages/testing/nros-tests/`. The low-level
`LifecyclePollingNodeCtx::register(slot, Option<extern fn>)` stays as the C-parity
seam the trait wraps.
