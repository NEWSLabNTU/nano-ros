# Phase 104 — Multi-RMW support + nros umbrella decoupling

**Goal.** Make the `nros` + `nros-node` umbrella crates fully
backend-agnostic at the Cargo / Rust API level so a single
binary can register and use multiple RMW backends (bridge nodes)
without compile-time mutual-exclusion. Three coupled threads
under one phase:

1. **API decoupling** — `nros` + `nros-node` carry no Rust deps
   on concrete RMW (`nros-rmw-zenoh`, `nros-rmw-dds`,
   `nros-rmw-xrce-cffi`) or platform (`nros-platform-posix`)
   crates. Core consumes only the generic ABI:
   `nros-rmw-cffi` vtable + `nros-platform-cffi` C header.
2. **Cargo feature elimination** — drop the per-backend
   `rmw-zenoh-cffi` / `rmw-dds-cffi` / `rmw-xrce-cffi` feature
   flags from `nros` + `nros-node`. Backend selection moves
   entirely to the outer build system (CMake) or to a thin
   loader crate (Rust).
3. **Multi-RMW runtime** — replace the singleton
   `static VTABLE` in `nros-rmw-cffi` with a per-process
   registry; per-session vtable pointer; opt-in
   `multi-backend` feature lifts the `compile_error!` mutex on
   `rmw-*` features.

**Status.** Not Started. Drafted 2026-05-13; rewritten
2026-05-14 to bundle the three threads.

**Priority.** Medium-High. Unblocks (a) PX4-on-drone bridge
(uORB ⇆ Zenoh), (b) ROS 2 cross-RMW gateways (XRCE ⇆ DDS),
(c) the "swap backend without rebuilding nros" promise the
phase 123 release-prep makes to users.

**Depends on.** Phase 102 (typed entity structs — reserved
`vtable` slot in `nros_rmw_session_t`). Phase 121 (canonical
platform-cffi). Phase 122 (handle ABI collapse — handles
already carry session refs). Phase 123.A.1.x (physical archive
split — prerequisite for "link backend at outer layer" to be
real). Phase 117 (RMW vtable surface frozen).

## Background

Today's nano-ros build picks one RMW backend at compile time.
Three load-bearing singletons enforce this:

1. **Cargo feature mutual-exclusion** —
   `compile_error!` in `nros/build.rs` if two `rmw-*` features
   are enabled.
2. **`ConcreteSession` type alias** in `nros-node` — collapses
   the executor to one Session type at compile time.
3. **`static VTABLE: AtomicPtr<NrosRmwVtable>`** in
   `nros-rmw-cffi/src/lib.rs:571` — one registered C backend
   per process.

This is load-bearing for embedded code-size (one backend's C
client trims 60–80 % of binary footprint vs upstream's
`dlopen`-style loader). But it forecloses three deployment
classes:

- **Cross-domain bridges** — uORB ⇆ Zenoh, XRCE ⇆ DDS.
- **Gradual migration** — running rclcpp legacy alongside
  nros for shared topics.
- **Backend swap without rebuild** — phase 123's release
  promise (`nano_ros_link_rmw(target xrce)` vs `zenoh`
  changes one line).

At the Rust API level, the umbrella `nros` + `nros-node` crates
still carry optional deps on the backend crates
(`nros-rmw-zenoh = { workspace = true, optional = true }`).
Each `rmw-<name>-cffi` feature pulls in the matching backend.
This forces every backend to live in the Cargo dep graph of the
umbrella, even when the user is a C/C++ caller who never
touches Cargo. Eliminating the features pushes backend
selection entirely to the outer build system, matching the
phase 123 `nano_ros_link_rmw(target zenoh)` model.

### The drone-bridge topology

```
[drone PX4 process]              [companion / cloud]
     uORB topics                       ROS 2 nodes
        ↓                                 ↑
   nros bridge ────── Zenoh ─────── zenohd ─────── rclcpp/rclrs
   (uORB sub +
    Zenoh pub)
```

The bridge subscribes to a small uORB topic set
(`vehicle_attitude`, `sensor_combined`,
`vehicle_local_position`, …) and republishes onto Zenoh keys
for the off-vehicle ROS 2 stack. Three reasons this needs
both backends in one binary:

1. **No agent in the middle.** `microxrcedds_agent` exists for
   the XRCE side; nothing equivalent for uORB. A bridge that
   lives inside or alongside PX4 is the cleanest path.
2. **Topic translation is the bridge's job.** PX4 doesn't
   speak Zenoh keys; the bridge maps uORB topic IDs ↔ ROS-2-
   style topic names.
3. **Single-binary deployment.** PX4 modules ship as one
   binary. Running two cooperating processes on flight
   hardware is a step backward.

## Design

### What stays the same

- **Trait surface unchanged.** `Rmw + Session + RmwConfig`
  already support multiple Session instances at the type
  level. No trait additions.
- **Single-backend builds unchanged.** Default Cargo features
  stay mutually exclusive; no code-size regression. Default
  binary = one linked backend C client, one registered
  vtable.
- **One `open()` call per session.** No adoption of
  upstream's `init_options_init` → `init` two-step. The
  three-call dance is upstream working around C's lack of
  constructors; struct-out-param C + Rust constructors don't
  need it.
- **No `implementation_identifier` per entity.**
  Monomorphisation already catches cross-backend wiring at
  compile time (`Executor<UorbSession>` cannot accept a
  `ZenohPublisher`). The runtime identifier would cost a
  pointer per entity for a use case the type system covers.

### What changes — three threads

#### Thread A — API decoupling (`nros` + `nros-node` cleanup)

`nros/Cargo.toml`:

```toml
# Today
[dependencies]
nros-rmw-zenoh    = { workspace = true, optional = true }
nros-rmw-dds      = { workspace = true, optional = true }
nros-rmw-xrce-cffi = { workspace = true, optional = true }

[features]
rmw-zenoh-cffi = ["rmw-cffi", "dep:nros-rmw-zenoh", "nros-node/rmw-zenoh-cffi"]
rmw-dds-cffi   = ["rmw-cffi", "dep:nros-rmw-dds",   "nros-node/rmw-dds-cffi"]
rmw-xrce-cffi  = ["rmw-cffi", "dep:nros-rmw-xrce-cffi", "nros-node/rmw-xrce-cffi"]
```

After:

```toml
[dependencies]
nros-rmw       = { workspace = true }
nros-rmw-cffi  = { workspace = true, optional = true }
# zero deps on concrete RMW crates

[features]
rmw-cffi = ["dep:nros-rmw-cffi", "nros-node/rmw-cffi"]
# no per-backend feature flags
```

`nros-node/Cargo.toml` — same shape: drop the three
optional deps + three feature flags.

Net effect: `nros` and `nros-node` describe the **generic
RMW ABI consumer**; backend selection happens entirely
outside their Cargo graph.

`nros-platform/src/resolve.rs:86` — the one remaining
Rust-side platform leak:

```rust
#[cfg(feature = "platform-posix")]
pub use nros_platform_posix::net::{
    NET_ENDPOINT_ALIGN, NET_ENDPOINT_SIZE,
    NET_SOCKET_ALIGN, NET_SOCKET_SIZE,
};
```

Moved either to `<nros/platform.h>` as `#define
NROS_PLATFORM_NET_SOCKET_SIZE 120` (compile-time, platform
binary supplies via header) or to a probe symbol
(`extern "C" const size_t NROS_PLATFORM_NET_SOCKET_SIZE`).
Header-supplied is simpler; matches the rest of the
canonical platform ABI surface.

#### Thread B — Backend registration model

With the Cargo features gone, "how does the user's binary
get the backend's vtable registered?" splits per audience:

**C / C++ via CMake.**

```cmake
find_package(NanoRos REQUIRED)
add_executable(my_node main.c)
nano_ros_link_platform(my_node)            # picks one platform
nano_ros_link_rmw(my_node zenoh)           # picks one RMW
# Bridge variant:
nano_ros_link_rmw(my_node zenoh xrce)      # both linked
```

The CMake helper:

- Links `libnros_rmw_<name>.a` with `--whole-archive` so the
  `__attribute__((constructor))` auto-register fn survives
  dead-strip.
- On bare-metal (no `.init_array` walk in libc startup),
  generates a tiny `rmw_register_<name>.c` stub that nros's
  `nros_init` calls explicitly.

User writes zero registration code.

**Rust via Cargo.**

Backend ships a thin **loader crate**:

```toml
# user Cargo.toml
[dependencies]
nros                       = "0.2"
nros-rmw-zenoh-loader      = "0.2"      # zero API; only purpose = ctor
```

Loader body:

```rust
#[ctor::ctor]
fn _register() {
    unsafe extern "C" { fn nros_rmw_register_zenoh(); }
    unsafe { nros_rmw_register_zenoh() };
}
```

The Cargo dep keeps the symbol alive; `#[ctor]` fires before
`main`. User writes zero registration code, same UX as
today's feature flag without the feature flag.

**Bare-metal / RTIC (no `.init_array`).**

Explicit call:

```rust
unsafe extern "C" { fn nros_rmw_register_zenoh(); }
fn main() {
    unsafe { nros_rmw_register_zenoh() };
    nros::init(cfg).unwrap();
}
```

CMake helper auto-injects the equivalent C stub when the
target's `<nros/platform.h>` indicates a no-libc-init
platform.

#### Thread C — Multi-RMW runtime

Three changes:

1. **`multi-backend` Cargo feature on `nros`.** Lifts the
   `compile_error!` mutual-exclusion check on the four
   `rmw-*` features (after Thread A: now expressed as
   multiple loader crates in the dep graph). Default off.
   Opting in accepts the code-size cost (each backend's C
   client linked).

2. **Registry replaces singleton vtable in `nros-rmw-cffi`.**

   ```rust
   // Replace:
   static VTABLE: AtomicPtr<NrosRmwVtable> = ...;

   // With:
   const MAX_BACKENDS: usize = 4;
   static REGISTRY: Mutex<heapless::Vec<Backend, MAX_BACKENDS>> = ...;

   struct Backend {
       name: &'static str,
       vtable: *const NrosRmwVtable,
   }

   #[unsafe(no_mangle)]
   pub unsafe extern "C" fn nros_rmw_cffi_register_named(
       name: *const c_char,
       vtable: *const NrosRmwVtable,
   ) -> NrosRmwRet {
       REGISTRY.lock().push(Backend { name, vtable })?;
       NROS_RMW_RET_OK
   }
   ```

   Backend ctors call `_register_named("zenoh", &VTABLE_Z)`
   independently — no clobber. Existing single-arg
   `nros_rmw_cffi_register(vtable)` kept as a shim that
   forwards to `_register_named("default", vtable)`, so
   single-backend builds need no source change.

3. **Per-session vtable pointer in `nros_rmw_session_t`.**
   Phase 102 already reserved a `void *backend_data` slot;
   this work adds a sibling `const NrosRmwVtable *vtable`.
   Every dispatch threads through `session->vtable->fn(...)`
   instead of `VTABLE->fn(...)`. Same indirect-call cost.

   `Support::open_with_rmw(name, locator, domain)` /
   `nros_support_init_with_rmw(...)` look up the vtable in
   the registry and bind it to the session at open time.

4. **`Executor::open_with_session(session, cfg)` constructor.**
   Bypasses the `ConcreteSession` type alias. Bridge code:

   ```rust
   let z_sess = ZenohRmw::default().open(&z_cfg)?;
   let x_sess = XrceRmw::default().open(&x_cfg)?;

   let mut node_z = Node::new(&z_sess, "bridge_pub")?;
   let mut node_x = Node::new(&x_sess, "bridge_sub")?;

   let pub_z = node_z.create_publisher::<Int32>("/forwarded")?;
   let mut exec = Executor::new(...)?;
   exec.register_subscription(&mut node_x, "/source",
       move |msg: &Int32| { pub_z.publish(msg).ok(); })?;

   loop { exec.spin_once(Duration::from_millis(100))?; }
   ```

   Existing `Executor::open` shorthand stays for the single-
   backend convenience case.

### Per-node vs per-entity model

**Per-node** (recommended). Each `Node` binds to one
backend at construction; bridge code creates one Node per
backend. Matches ROS 2 mental model (context per node).
Cross-backend wiring fails at compile time in Rust
(`Node<S>` monomorphisation); C/C++ users wire by
convention.

Per-entity (one Node spans backends) rejected: loses Node
identity invariant (discoverability and lifecycle scope
per-backend), and breaks rclcpp/rclrs migration parity.

### Executor model

Two viable shapes; pick (B) for the long term:

**A. Per-backend Executor.** Each backend gets its own
`Executor<S>`; bridge code round-robins
`exec_z.spin_once(10); exec_x.spin_once(10);`. Zero
executor internals change. Two wait condvars.

**B. Single Executor, mixed handles.** Each handle carries
its session ref (already true post-phase-122 — handle
storage is opaque, session pointer lives inside). Executor
dispatches via per-handle vtable. One spin loop, one
wakeup. Arena heterogeneity is invisible to executor
internals since dispatch is per-handle.

**Recommend B** because phase 122's opaque handles already
collapsed session ownership into handle storage; the
executor never sees session types directly. Adding mixed
backends is a no-op at the executor level.

## Memory + code-size budget

Multi-backend cost on a companion-class target (Jetson Orin
/ Raspberry Pi):

| Component | Flash | Heap |
|-----------|-------|------|
| zenoh-pico C client | ~80 KB | ~64 KB |
| uORB rmw (intra-process) | ~5 KB | ~0 |
| nros runtime + executor | ~30 KB | per-arena |
| Bridge logic | trivial | trivial |
| **Total** | **~115 KB Flash, ~64 KB heap** | comfortable |

On a Cortex-M4 with 256 KB Flash + 128 KB SRAM: tight but
feasible (zenoh-pico's TLS feature stays off). On a
Cortex-M0+: not viable — code size alone breaks the budget.

Validates the opt-in design: default builds unchanged,
only binaries that explicitly opt in pay the cost.

## Work Items

Three threads run in order. Threads A + B are cleanup that
makes Thread C natural; Thread C is the user-facing
deliverable.

### Thread A — API decoupling

- [ ] **104.A.1 — Drop concrete RMW deps from `nros`.**
      Remove `nros-rmw-zenoh`, `nros-rmw-dds`,
      `nros-rmw-xrce-cffi` from `[dependencies]` in
      `packages/core/nros/Cargo.toml`. Remove
      `rmw-zenoh-cffi`, `rmw-dds-cffi`, `rmw-xrce-cffi`
      feature flags. Keep `rmw-cffi` (the generic ABI
      feature). Update `Cargo.toml` consumers (testing
      crates, examples) that referenced the per-backend
      features.
      **Files:** `packages/core/nros/Cargo.toml`,
      `packages/core/nros/build.rs`,
      consumer Cargo.toml files (sweep via `grep`).

- [ ] **104.A.2 — Drop concrete RMW deps from `nros-node`.**
      Same as 104.A.1 for `nros-node`. Audit
      `packages/core/nros-node/src/session.rs` for the
      `ConcreteSession` cfg cascade; it stays (Thread C
      replaces it later) but stops referencing the per-
      backend features.
      **Files:** `packages/core/nros-node/Cargo.toml`,
      `packages/core/nros-node/src/session.rs`.

- [ ] **104.A.3 — Move platform net-size constants to
      `<nros/platform.h>`.** Drop the
      `#[cfg(feature = "platform-posix")] pub use
      nros_platform_posix::net::{...}` re-export at
      `packages/core/nros-platform/src/resolve.rs:86`.
      Replace with `extern "C"` const lookup or move sizes
      into the canonical header as `#define`s. Bare-metal
      fallback (64-byte) becomes the default; POSIX
      publishes the real sizes via the header.
      **Files:**
      `packages/core/nros-platform/src/resolve.rs`,
      `packages/core/nros-platform-cffi/include/nros/platform.h`,
      `packages/core/nros-platform-posix-c/include/...` (if
      needed).

- [ ] **104.A.4 — Verify generic-only Cargo graph.**
      Audit `cargo tree -p nros --no-default-features
      --features rmw-cffi` shows no
      `nros-rmw-{zenoh,dds,xrce-cffi}` / `nros-platform-
      <concrete>` in the tree. Add as a CI guard
      (`just check-decoupling` or similar).
      **Files:** `justfile`, `.github/workflows/*.yml` if CI.

### Thread B — Backend registration model

- [ ] **104.B.1 — `nros_rmw_cffi_register_named(name,
      vtable)`.** New C entry point. Existing
      `nros_rmw_cffi_register(vtable)` shim calls
      `_register_named("default", vtable)` for source
      compatibility. Registry holds up to
      `MAX_BACKENDS = 4`.
      **Files:** `packages/core/nros-rmw-cffi/src/lib.rs`,
      `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`.

- [ ] **104.B.2 — Backend ctors call `_register_named`.**
      Each backend cffi shim
      (`nros-rmw-zenoh`, `nros-rmw-dds`,
      `nros-rmw-xrce-cffi`) gets a ctor that calls
      `_register_named("<backend-name>", &VTABLE)`. POSIX +
      ESP-IDF + Zephyr platforms run `.init_array`
      automatically; bare-metal platforms get explicit
      `nros_rmw_register_<name>()` C entry points called
      from `nros_init`.
      **Files:** each backend's `src/lib.rs` ctor section.

- [ ] **104.B.3 — Rust loader crates.** New crates:
      `nros-rmw-zenoh-loader`,
      `nros-rmw-dds-loader`,
      `nros-rmw-xrce-cffi-loader`. Each is a thin shim with
      one `#[ctor::ctor]` fn that calls the matching
      `nros_rmw_register_<name>()`. No public API beyond
      the linker-attribute fn.
      **Files:** `packages/core/nros-rmw-<name>-loader/`
      (new crates).

- [ ] **104.B.4 — CMake `nano_ros_link_rmw` survives
      Thread A.** Phase 123.A.6 already shipped this
      helper, but Thread A removes the Cargo features
      it currently bridges to. Update the helper to
      directly select `libnros_rmw_<name>.a` from the
      installed archives (phase 123.A.1.x.{2..4}
      produced these). Add `--whole-archive` around the
      static archive so the ctor symbol survives dead-
      strip.
      **Files:**
      `cmake/NanoRosLink.cmake`,
      `cmake/NanoRosCTargets.cmake.in`.

- [ ] **104.B.5 — Bare-metal explicit-call generator.**
      For platforms where `.init_array` doesn't run
      automatically (FreeRTOS, NuttX, ThreadX, RTIC
      Cortex-M), `nano_ros_link_rmw(target name)` emits
      a tiny stub `rmw_register_<name>.c` that calls
      `nros_rmw_register_<name>()`. `nros_init` invokes
      a weak-symbol fan-out; backends provide the strong
      definition.
      **Files:**
      `cmake/NanoRosLink.cmake`,
      `packages/core/nros-c/src/init.rs`.

### Thread C — Multi-RMW runtime

- [ ] **104.C.1 — Per-session vtable pointer.**
      Embed `vtable: *const NrosRmwVtable` in
      `nros_rmw_session_t` (C) / `NrosRmwSession`
      (Rust). All dispatch sites (`session->fn(...)`)
      switch from reading the static `VTABLE` to
      reading `session->vtable`. Single-backend dispatch
      cost unchanged (still one indirect call). Phase
      102's reserved slot makes this a one-pointer
      addition.
      **Files:**
      `packages/core/nros-rmw-cffi/include/nros/rmw_entity.h`,
      `packages/core/nros-rmw-cffi/src/lib.rs`.

- [ ] **104.C.2 — `Support::open_with_rmw(name, locator,
      domain)`.** New Rust + C + C++ entry points. Look
      up the named backend in the registry; bind the
      vtable to the session at open time.
      `Support::open(locator, domain)` (no name) falls
      back to the single registered backend (no name
      lookup; fast path).
      **Files:**
      `packages/core/nros-node/src/support.rs`,
      `packages/core/nros-c/src/support.rs`,
      `packages/core/nros-cpp/include/nros/support.hpp`.

- [ ] **104.C.3 — `multi-backend` Cargo feature.** Add
      a `multi-backend` feature on `nros` that lifts the
      `compile_error!` mutual-exclusion check on the
      four `rmw-*` features. Default off. Audit the
      codebase for any other assumptions of single-
      backend (build.rs cfg emissions, type aliases,
      etc.) and feature-gate them appropriately.
      **Files:**
      `packages/core/nros/Cargo.toml`,
      `packages/core/nros/build.rs`,
      `packages/core/nros-node/src/session.rs` (the
      `ConcreteSession` alias's `cfg` block).

- [ ] **104.C.4 — `Executor::open_with_session`.** New
      constructor that takes an already-opened Session
      by value. Existing `Executor::open` stays — it
      constructs the `ConcreteSession` from `RmwConfig`
      and calls the new path. Document the convention:
      single-backend apps use `open()`, multi-backend
      bridge apps use `open_with_session()`.
      **Files:**
      `packages/core/nros-node/src/executor/mod.rs`,
      `packages/core/nros-node/src/executor/session.rs`.

- [ ] **104.C.5 — Drop static `VTABLE`.** Once 104.C.1
      lands and all dispatch threads through
      `session->vtable`, delete the singleton in
      `packages/core/nros-rmw-cffi/src/lib.rs:571`. The
      `nros_rmw_cffi_register` shim becomes a thin
      wrapper around `_register_named` (104.B.1). Extend
      the typed-struct roundtrip test (Phase 102.5) to
      drive two simultaneous sessions with two stub
      vtables.
      **Files:**
      `packages/core/nros-rmw-cffi/src/lib.rs`,
      `packages/core/nros-rmw-cffi/tests/typed_struct.rs`.

### Thread D — Validation

- [ ] **104.D.1 — Bridge example (uORB → Zenoh).**
      `examples/native/rust/bridge/uorb-to-zenoh/`.
      Subscribes to `vehicle_attitude`,
      `sensor_combined`,
      `vehicle_local_position` via `nros-rmw-uorb`;
      republishes onto Zenoh keys via
      `nros-rmw-zenoh`. Built against PX4 SITL via the
      Phase 98 fixture. Topic-name translation table
      embedded as a `phf` perfect-hash so adding a new
      uORB ↔ Zenoh mapping is one table-row change.
      **Files:**
      `examples/native/rust/bridge/uorb-to-zenoh/`
      (new crate), workspace `Cargo.toml` exclude.

- [ ] **104.D.2 — Bridge example (XRCE ⇆ DDS).** Same
      shape, C audience.
      `examples/native/c/bridge/xrce-to-dds/`.
      Demonstrates the CMake `nano_ros_link_rmw(target
      xrce dds)` two-backend link.
      **Files:**
      `examples/native/c/bridge/xrce-to-dds/`.

- [ ] **104.D.3 — Bridge E2E test.**
      `packages/testing/nros-tests/tests/bridge_uorb_to_zenoh.rs`.
      Boots PX4 SITL via Phase 98's `Px4Sitl::boot_in()`
      fixture, runs the bridge example, runs a host-side
      rclcpp listener via the existing ROS 2 interop
      fixture, asserts ≥ 80 % message delivery on at
      least one topic in a 10 s window.
      **Files:**
      `packages/testing/nros-tests/tests/bridge_uorb_to_zenoh.rs`,
      `.config/nextest.toml` (slow-timeout group).

- [ ] **104.D.4 — Decoupling CI guard.** `just check-
      decoupling`: `cargo tree -p nros
      --no-default-features --features rmw-cffi` must
      show zero entries matching `nros-rmw-
      (zenoh|dds|xrce|cyclonedds)` and zero entries
      matching `nros-platform-(posix|freertos|nuttx|
      threadx|zephyr|esp)`. Wire into CI.
      **Files:** `justfile`,
      `.github/workflows/check.yml`.

- [ ] **104.D.5 — Book chapter.**
      `book/src/user-guide/cross-backend-bridges.md`.
      Covers the bridge topology, the `multi-backend`
      Cargo feature flag, the registration model
      per audience, the memory-budget table, and the
      bridge example walkthrough. Cross-link from
      `book/src/concepts/ros2-comparison.md` ("backend
      selection at compile time" section) and from
      `examples/README.md`.
      **Files:**
      `book/src/user-guide/cross-backend-bridges.md`,
      `book/src/SUMMARY.md`,
      `book/src/concepts/ros2-comparison.md`.

## Acceptance Criteria

### API decoupling

- [ ] `cargo tree -p nros --no-default-features --features
      rmw-cffi` shows no concrete RMW or platform crates.
- [ ] `cargo tree -p nros-node --no-default-features
      --features rmw-cffi` same.
- [ ] `nros-platform/src/resolve.rs` has no
      `cfg(feature = "platform-posix")` block referencing
      `nros_platform_posix::net`.

### Feature elimination

- [ ] `packages/core/nros/Cargo.toml` has no
      `rmw-zenoh-cffi` / `rmw-dds-cffi` / `rmw-xrce-cffi`
      feature flags.
- [ ] User builds via CMake unchanged
      (`nano_ros_link_rmw(target zenoh)` works).
- [ ] Rust users either depend on the `*-loader` crate or
      call `nros_rmw_register_<name>()` explicitly; old
      Cargo-feature builds emit a deprecation warning for
      at least one minor release before final removal.

### Multi-RMW runtime

- [ ] `nros` builds clean with
      `--features rmw-cffi,multi-backend` plus two loader
      crates in the user's Cargo graph on POSIX.
- [ ] Default builds (no `multi-backend`) still fail at
      compile time when two `rmw-*` features (now expressed
      as loader-crate deps) are enabled — the mutual-
      exclusion check stays on by default.
- [ ] `Executor::<UorbSession>::open_with_session` and
      `Executor::<ZenohSession>::open_with_session` coexist
      in one binary (verified by the bridge example crate).
- [ ] `nros-rmw-cffi` no longer holds a global `VTABLE`.
      Two simultaneous `CffiSession::open` calls with
      different stub vtables both succeed (verified by an
      extension to `tests::typed_struct_roundtrip`).
- [ ] PX4 SITL bridge E2E test green: ≥ 80 % delivery on
      `vehicle_attitude` over 10 s.
- [ ] Book chapter renders clean (`mdbook build`).
- [ ] No regression in any single-backend test suite (full
      `just test` green).

## Notes

- **Why opt-in instead of always-on?** Code-size: each
  linked backend adds 5–80 KB Flash. Embedded users running
  a single backend don't want to pay for runtime backend-
  selection plumbing they'll never use. Default-off keeps
  the smallest targets cheap.
- **Why not adopt upstream's `rmw_init_options_t` +
  `rmw_context_t` split?** Our `RmwConfig` + `Session`
  already covers the same ground in fewer steps (one
  constructor instead of three). The three-call dance is
  upstream working around C's lack of constructors; we have
  Rust + a struct-out-param C calling convention, so we
  don't need it. Multi-instance doesn't require multi-step
  init.
- **Why not adopt `implementation_identifier`?** Upstream's
  cross-backend identifier check defends against plugin-
  loader-induced confusion (every entity is opaque
  `rmw_publisher_t *`, implementation-agnostic). Our typed-
  with-monomorphisation model catches the same mistakes at
  compile time — `Executor<UorbSession>` cannot accept a
  `ZenohPublisher` by type-system construction. The runtime
  identifier would add a pointer per entity for a use case
  our type system already covers.
- **Cross-backend bridges with three+ backends.** Out of
  scope. If someone needs uORB + Zenoh + XRCE in one binary,
  the same pattern extends — three loader crates under
  `multi-backend`, one shared executor with handles spanning
  all three sessions. The work in this phase is the
  *enablement*; the combinatorics are the user's problem.
- **Hot-path latency.** The bridge runs the executor's spin
  loop driving both Sessions' I/O. For a 100 Hz uORB topic
  going to a 100 Hz Zenoh peer, the bridge adds one
  re-publish hop = one CDR encode + one Zenoh `z_put`. On a
  Jetson-class CPU this is sub-millisecond per sample;
  uORB→Zenoh end-to-end latency is dominated by Zenoh
  scout/routing on the egress side, not by the bridge.
- **Phase 123 interaction.** Phase 123.A.1.x splits the
  physical archives; Thread A here removes the Rust-side
  Cargo deps that still pin the backends into `nros`. Both
  must land before the "swap backend without rebuilding
  nros" promise is real. Sequence: 123.A.1.x → 104.A →
  104.B → 104.C → 104.D.
- **`compile_error!` mutex removal.** Phase 104.C.3 lifts
  the mutex behind `multi-backend`. Bridge nodes are the
  driver; the same lift accidentally enables future "two
  zenoh sessions on different domains" use cases, which is
  fine.
