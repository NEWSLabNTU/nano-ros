# Opaque Storage Sizing

`nano-ros` exposes a C and C++ API on top of Rust types whose layout is
chosen by the Rust compiler. The C/C++ side has to allocate storage for
those types — by value, on the user's stack or BSS — without knowing
their exact byte size. The runtime makes Rust the single source of
truth for those sizes: the values flow from `core::mem::size_of` of
the real Rust types into auto-generated C/C++ headers, with no
hand-tuned constants.

## The pattern

Every size that crosses the Rust / C boundary follows the same path:

1. **Export from `nros::sizes`** — the `nros` umbrella crate defines a
   `pub const FOO_SIZE: usize = core::mem::size_of::<T>();` and emits a
   `#[used] static __NROS_SIZE_FOO: [u8; FOO_SIZE] = [0u8; FOO_SIZE]`
   with a `#[no_mangle]` symbol whose storage size in the rlib *is*
   `FOO_SIZE`. The two artefacts come from a single `export_size!` macro
   invocation:

   ```rust
   // packages/core/nros/src/sizes.rs
   export_size!(pub PUBLISHER_SIZE = RmwPublisher);
   export_size!(pub SUBSCRIBER_SIZE = RmwSubscriber);
   export_size!(pub EXECUTOR_SIZE = nros_node::Executor);
   // ...etc
   ```

2. **Probe from consumer build scripts** — `nros-c/build.rs` and
   `nros-cpp/build.rs` use the helper crate `nros-sizes-build` to find
   the compiled `nros` rlib (via `cargo metadata` + a glob over
   `target/<triple>/<profile>/deps/`) and read the `__NROS_SIZE_*`
   symbol storage sizes with the [`object`](https://crates.io/crates/object)
   crate. No subprocess, no llvm-nm; pure Rust.

3. **Emit `#define NROS_FOO_SIZE` into a generated header** —
   `nros_config_generated.h` (C) and `nros_cpp_config_generated.h` (C++)
   carry the probe values. `types.h` includes the generated config
   transitively, so every nros C header sees `NROS_*_SIZE` automatically.

4. **C/C++ structs use the macros** —

   ```c
   typedef struct nros_publisher_t {
       /* ... */
       _Alignas(8) uint8_t _opaque[NROS_PUBLISHER_SIZE];
   } nros_publisher_t;
   ```

   ```cpp
   class Publisher {
       alignas(8) uint8_t storage_[NROS_PUBLISHER_SIZE];
       /* ... */
   };
   ```

   The Rust side reads/writes the same bytes via
   `&mut *(opaque as *mut RmwPublisher)`. C and Rust agree on the size by
   construction.

## What the SSoT covers today

The `nros::sizes` module exports:

| Symbol | Type | Used by |
|---|---|---|
| `SESSION_SIZE` | `RmwSession` | `nros_support_t._opaque` |
| `PUBLISHER_SIZE` | `RmwPublisher` | `nros_publisher_t._opaque`, `nros::Publisher<M>::storage_` |
| `SUBSCRIBER_SIZE` | `RmwSubscriber` | `nros::Subscription<M>::storage_` |
| `SERVICE_CLIENT_SIZE` | `RmwServiceClient` | `nros::Client<S>::storage_` |
| `SERVICE_SERVER_SIZE` | `RmwServiceServer` | `nros::Service<S>::storage_` |
| `EXECUTOR_SIZE` | `nros_node::Executor` | `nros_executor_t._opaque`, `nros::Executor::storage_` |
| `GUARD_CONDITION_SIZE` | `nros_node::GuardConditionHandle` | `nros_guard_condition_t._guard_opaque`, `nros::GuardCondition::storage_` |
| `LIFECYCLE_CTX_SIZE` | `nros_node::lifecycle::LifecyclePollingNodeCtx` | `nros_lifecycle_state_machine_t._opaque_storage` |
| `ACTION_SERVER_INTERNAL_SIZE` | `ActionServerInternalLayout` | `nros_action_server_t._internal` |
| `CPP_ACTION_SERVER_SIZE` | `CppActionServerLayout` | `nros::ActionServer<A>::storage_` |
| `CPP_ACTION_CLIENT_SIZE` | `CppActionClientLayout` | `nros::ActionClient<A>::storage_` |

Plus three `*Internal` C-API shim structs (`ServiceServerInternal`,
`ServiceClientInternal`, `ActionClientInternal`) that cbindgen now
emits directly into `nros_generated.h` because they're `#[repr(C)]` —
the C side just embeds them as typed fields, no opaque storage at all.

## Layout-mirror trick

Some downstream types — `nros-c::ActionServerInternal`,
`nros-cpp::CppActionServer`, `nros-cpp::CppActionClient` — embed C-API
pointer types (`*mut nros_action_server_t`, `*const nros_goal_handle_t`)
that aren't visible from the `nros` umbrella crate. They can't be
referenced from `nros::sizes` directly, but their byte size only
depends on the field shape: pointers are pointers, `Option<extern "C"
fn>` collapses to a fn-pointer-sized slot via niche optimization, etc.

The fix is a layout-mirror struct in `nros::sizes`:

```rust
// packages/core/nros/src/sizes.rs
#[repr(C)]
#[doc(hidden)]
pub struct ActionServerInternalLayout {
    pub handle: nros_node::ActionServerRawHandle,
    pub executor_ptr: *mut c_void,
    pub c_goal_callback: unsafe extern "C" fn(
        *mut c_void, *const c_void, *const u8, usize, *mut c_void,
    ) -> i32,
    // ...same field shape as the real `ActionServerInternal`,
    // with C-API pointers replaced by `*mut c_void`...
}
export_size!(pub ACTION_SERVER_INTERNAL_SIZE = ActionServerInternalLayout);
```

Downstream then asserts byte-equivalence at compile time:

```rust
// packages/core/nros-c/src/opaque_sizes.rs
const _: () = assert!(
    size_of::<crate::action::ActionServerInternal>()
        == size_of::<nros::sizes::ActionServerInternalLayout>(),
    "ActionServerInternal size diverges from nros::sizes::ActionServerInternalLayout — \
     update the layout mirror in `nros/src/sizes.rs`"
);
```

The tripwire: any field-shape change in the real wrapper (adding a
field, changing a pointer to a value, etc.) must be paired with an
update to the mirror. The build fails immediately if they diverge.

## How sizing works today

- All four `*Internal` shim types are `#[repr(C)]` and embedded as
  typed fields in their outer `nros_*_t` structs.
- All seven C++ wrappers (Publisher, Subscription, Service, Client,
  ActionServer, ActionClient, GuardCondition) use the
  `NROS_*_SIZE` / `NROS_CPP_*_STORAGE_SIZE` probe macros.
- `types.h` ships zero `*_OPAQUE_U64S` macros; the four consumer
  module headers route through the probe.
- The remaining hand-coded "upper bound" assertions (e.g., the
  `EXECUTOR_OPAQUE_U64S` envelope check in `nros-c/build.rs`) are
  defence-in-depth: they fire if a Rust type accidentally exceeds a
  configured envelope (which would normally also be flagged by the
  config knobs).

## Adding a new size export

When a new Rust handle type needs to cross the FFI boundary:

1. Add the `export_size!(pub FOO_SIZE = nros_node::Foo)` line to
   `nros/src/sizes.rs`. (If `Foo` lives in a downstream crate that
   `nros` can't import, define a `FooLayout` mirror struct first and
   add a byte-equivalence assert in the owning crate.)
2. Add a `let probe_foo = probed.get("FOO_SIZE").copied().unwrap_or(0)`
   line to `nros-c/build.rs` (and/or `nros-cpp/build.rs`), and emit
   `#define NROS_FOO_SIZE {probe_foo}` into the generated config
   header.
3. Use `NROS_FOO_SIZE` in the C/C++ struct that needs the storage.

Cross-target verification: the same probe runs for every target
triple. There's no per-platform branching — `size_of::<T>()` resolves
to the right value at the target's compile time.

## See also

- `packages/core/nros-sizes-build/src/lib.rs` for the rlib probe
  implementation.
- `packages/core/nros/src/sizes.rs` for the canonical exports.
