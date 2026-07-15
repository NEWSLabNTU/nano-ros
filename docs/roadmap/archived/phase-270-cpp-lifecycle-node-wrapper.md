# Phase 270 — C++ `nros::LifecycleNode` wrapper (rclcpp-shape managed nodes)

Status: **DONE (2026-07-02)** — see Outcome at the end.

Implements **[#103](../issues/0103-cross-language-capability-surface-gaps.md)** (its one surviving
hard gap — C++ lifecycle has no idiomatic wrapper class). Realizes the C++ side of **RFC-0019**
(nros-c thin-wrapper discipline: the C++ class is a thin, allocation-free wrapper over the complete
C/Rust state machine, adding no logic). Complements **#117 / phase-269** (which added *entry-level*
`[lifecycle]` autostart codegen) by giving a C++ author the *user-facing* API to write a managed
node's transition behavior — the piece #117 did not provide.

## Why

Re-audit of #103 (2026-07-01) confirmed two of its three original hard gaps were already closed
(multi-type params — Phase 91.C/117.9; RT tiers — Phase 110.B). The **one genuine remainder**:
`nros-cpp` ships no lifecycle wrapper class. A C++ managed node that wants to author
`on_configure` / `on_activate` behavior must drop to the `extern "C"` `nros_executor_lifecycle_*`
C-ABI (raw `void*` executor, `uint8_t` transition ids) — asymmetric with how C++ wraps every other
capability in a class, and thinner than the C side (the C++ FFI shim from phase-269 exposes
`register_lifecycle_services` + `change_state` + `autostart` but **not** `get_state` or the
`register_on_*` transition callbacks).

The C side is already complete (`nros_executor_lifecycle_*` in `nros_generated.h`). C has no
classes, so its fn-ptr registration *is* the C analog and is unchanged. This phase closes the
**C++** surface only.

## Design (approved)

rclcpp-faithful shape (`rclcpp_lifecycle::node_interfaces::LifecycleNodeInterface`): inherit a base
class, override `on_*` virtuals returning `CallbackReturn`. Freestanding-safe — `nros-cpp` already
uses non-pure virtuals with defaults under `-ffreestanding -nostdinc++` (`ComponentNode`,
`TimerBase` dtors), so no RTTI / exceptions / `__cxa_pure_virtual`.

### W1 — Rust FFI (no_std), `packages/core/nros-cpp/src/lifecycle_shim.rs`

Add, gated `#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]` like the existing
shim (no_std, `core::ffi::c_void`), recovering `CppContext → &mut Executor → service-backed
lifecycle SM` (the same path `nros_cpp_lifecycle_change_state` already uses):

- `nros_cpp_lifecycle_get_state(executor: *mut c_void) -> u8`
- `nros_cpp_lifecycle_register_on_configure(executor: *mut c_void, cb: extern "C" fn(*mut c_void) -> u8, ctx: *mut c_void) -> nros_cpp_ret_t`
  and the same for `_on_{activate,deactivate,cleanup,shutdown,error}` (6 total).

Each register shim forwards to the executor's service-backed `lifecycle_register_on_*` ctx-callback
method (the one the C FFI `nros_executor_lifecycle_register_on_*` already calls). Null-executor →
`NROS_CPP_RET_INVALID_ARGUMENT`; SM absent → `NROS_CPP_RET_NOT_INIT`. cbindgen re-exports the decls
into `nros_cpp_ffi.h`. Unit test: null-executor guard for every new fn (mirror the existing shim
test).

### W2 — C++ header, `packages/core/nros-cpp/include/nros/lifecycle.hpp`

Freestanding (only `<cstdint>` + `nros_cpp_ffi.h` types + `nros::Result`, like `sched_context.hpp`;
no `<string>`, no exceptions, no RTTI):

```cpp
namespace nros {
enum class LifecycleState : uint8_t { Unconfigured = 0, Inactive = 1, Active = 2, Finalized = 3 };
enum class CallbackReturn : uint8_t { Success = 0, Failure = 1, Error = 2 };   // rclcpp shape

class LifecycleNode {
public:
  explicit LifecycleNode(void* executor_handle) : exec_(executor_handle) {}
  virtual ~LifecycleNode() = default;

  virtual CallbackReturn on_configure(LifecycleState previous)  { (void)previous; return CallbackReturn::Success; }
  virtual CallbackReturn on_activate(LifecycleState previous)   { (void)previous; return CallbackReturn::Success; }
  virtual CallbackReturn on_deactivate(LifecycleState previous) { (void)previous; return CallbackReturn::Success; }
  virtual CallbackReturn on_cleanup(LifecycleState previous)    { (void)previous; return CallbackReturn::Success; }
  virtual CallbackReturn on_shutdown(LifecycleState previous)   { (void)previous; return CallbackReturn::Success; }
  virtual CallbackReturn on_error(LifecycleState previous)      { (void)previous; return CallbackReturn::Failure; }

  Result register_services();                  // services + bind the 6 on_* trampolines
  Result autostart(LifecycleState target);     // services + drive to Inactive/Active
  LifecycleState get_state() const;
  Result configure(){return trigger(1);}  Result activate(){return trigger(2);}
  Result deactivate(){return trigger(3);} Result cleanup(){return trigger(4);} Result shutdown(){return trigger(5);}
protected:
  Result trigger(uint8_t transition_id);
  void* exec_;
private:
  static uint8_t tramp_configure(void* s){ auto* n=static_cast<LifecycleNode*>(s); return (uint8_t)n->on_configure(n->get_state()); }
  // …5 more. previous_state = get_state() at callback entry (SM still in the source state).
};
} // namespace nros
```

`register_services()` calls `nros_cpp_register_lifecycle_services` then binds each static trampoline
via the W1 `register_on_*` FFI with `ctx = this`. Trampolines fetch `previous = get_state()` and
dispatch to the virtual — no C-ABI callback-signature change (stays `fn(ctx) -> u8`).
`transition_id` ↔ REP-2002: Configure=1, Activate=2, Deactivate=3, Cleanup=4, Shutdown=5,
ErrorProcessed=6.

### W3 — e2e (prebuilt fixture; no compile-in-test)

A C++ managed-node fixture inheriting `nros::LifecycleNode`, overriding `on_configure` / `on_activate`
to flip observable flags (or publish on a topic). Boot it, drive `configure()` → `activate()` (or a
`ros2 lifecycle set` interop path against the registered REP-2002 services), and assert: the
overrides fired in order, `get_state() == Active`. Build-stage fixture + `nros-tests` consumer
(behavior-named, e.g. `cpp_lifecycle_node_wrapper_e2e`).

## Acceptance

- `nros::LifecycleNode` lets a C++ author write a managed node by inheriting + overriding the
  rclcpp-shape `on_*` hooks — no `extern "C"` in user code.
- `get_state()` + all six `on_*` callbacks reachable from C++; W3 e2e green.
- Header compiles under the freestanding embedded C++ profile (`-ffreestanding -nostdinc++`, no
  `<string>`/RTTI/exceptions); no new heap use.
- `just ci` green (incl. per-example clippy + the C++ build).
- #103 resolved (→ archived) once W3 is green.

## Reference — user-facing example

```cpp
#include <nros/lifecycle.hpp>
#include <nros/executor.hpp>

class TalkerLifecycle : public nros::LifecycleNode {
public:
  using nros::LifecycleNode::LifecycleNode;   // inherit ctor(void* executor_handle)
  nros::CallbackReturn on_configure(nros::LifecycleState) override { configured_ = true; return nros::CallbackReturn::Success; }
  nros::CallbackReturn on_activate(nros::LifecycleState)   override { active_ = true;     return nros::CallbackReturn::Success; }
  nros::CallbackReturn on_deactivate(nros::LifecycleState) override { active_ = false;    return nros::CallbackReturn::Success; }
  nros::CallbackReturn on_cleanup(nros::LifecycleState)    override { configured_ = false;return nros::CallbackReturn::Success; }
  bool configured_ = false, active_ = false;
};

// entry:
TalkerLifecycle node{exec.handle()};
node.register_services();                       // `ros2 lifecycle set … configure` now drives it
// or boot straight to active: node.autostart(nros::LifecycleState::Active);
```

## Out of scope (tracked in #103, minor)

- The phase-269 #116 component *live-read* param shim covers int/double/string only (no bool/array).
- C++ `get_logger()` returns an opaque handle vs Rust's `Logger` object (cosmetic).
- `LifecycleState::label()` string helper + `std::function` `on_*` overloads under `NROS_CPP_STD` —
  deferred (YAGNI); the enum + virtual-override surface is the contract.

## Outcome (2026-07-02) — DONE

All three waves landed + verified.
- **W1** — `nros_cpp_lifecycle_get_state` + 6 `nros_cpp_lifecycle_register_on_*` in
  `lifecycle_shim.rs` (no_std), cbindgen-rendered cleanly into `nros_cpp_ffi.h`.
- **W2** — `nros/lifecycle.hpp`: `LifecycleState` / `CallbackReturn` enums + the
  `LifecycleNode` base (rclcpp-shape virtuals, trampolines, `bind()` two-phase init).
  Fixed `LifecycleState` to the REP-2002 numbering (Unconfigured=1…Active=3…).
- **W3** — `ManagedTalker` (ws-lifecycle-cpp, `native_managed_entry` / `managed_bringup`,
  no `[lifecycle]` block — self-drives via the wrapper). `cpp_lifecycle_node_wrapper_e2e`
  is **GREEN**: the node reaches Active, `on_configure`/`on_activate` fire (trampolines
  dispatch to the overrides), `get_state()==Active`, and publishing is gated on the active
  state. Fixture build needed the config-header mirror (`nros_{c,cpp}_config_header`) built
  first — a known 0114-class ordering the workspace recipe handles.

Closes **[#103](../issues/archived/0103-cross-language-capability-surface-gaps.md)**: a C++
managed node is now authored by inheriting `nros::LifecycleNode` + overriding `on_*`, with no
`extern "C"` in user code.
