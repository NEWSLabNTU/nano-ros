# Phase 216 — Bare-metal Framework Integration (RTIC + Embassy)

**Goal.** `nros::main!()` works cleanly on RTIC + Embassy bare-metal targets
with the same one-line UX as POSIX + RTOS. Node pkgs declare a
`DispatchStrategy` and stay framework-portable; callbacks fire from the
framework's task scheduler (not nros's spin loop) when running on a
framework-aware board.

**Status.** Design locked 2026-06-03 (B+C composition; sync-only callbacks
for v1; tag-based registration API for Deferred Components). Spec
refreshed 2026-06-04 to match post-212.N.12 + post-214.K.1 trait
naming (see "Trait surface after 212.N.12 + 214.K.1" below).

**Status update 2026-06-04.** Substrate (216.A.1–A.5) + RTIC track
(216.B.1–B.5, all six examples migrated) + Embassy track
(216.C.1–C.5) + book chapters (216.D.2) all landed on
`feature/phase-216-frameworks`. Notable carve-outs:

* **B.3 / C.3 task bodies are placeholders.** The proc-macro emits
  the `__nros_spin{,_task}` + `__nros_dispatch{,_task}` task shells
  and the init/spawn wiring, but the dispatch-loop bodies still need
  the per-Node trampoline-registration story (linkme section walk +
  `__nros_node_<pkg>_on_callback` routing). Spin loop body is a stub
  awaiting the wiring.
* **B.5 example migrations** flip example shape to `nros::main!()` +
  carved `*_pkg/` Node packages but inherit the placeholder dispatch
  — they compile + boot but Deferred examples won't actually fire
  `on_callback` from the dispatch task until the routing body lands.
* **216.A.6 (closure-in-Deferred lint) deferred** — speculative; no
  concrete misuse pattern surfaced yet. Revisit when the first user
  hits the trap.
* **216.D.1 (`nros check` matrix lint) deferred** — lives in the
  standalone `nros-cli` repo (Phase 195.D retired in-tree codegen);
  landing it requires a coordinated bump there. Substrate ships the
  `__nros_node_<pkg>_dispatch_strategy` ABI so the lint has a hook to
  read from when it lands.
* **216.E.1 / E.2 / E.3** remain explicitly deferred per spec.

**Status update 2026-06-06.** 216.B validation is emulator-first. The
STM32F4 RTIC examples remain useful cross-compile portability coverage, but
they cannot be the end-to-end gate because this project only has emulator
hardware in CI/local validation. The E2E proof now targets QEMU MPS2-AN385:

* Add an RTIC board-entry crate for MPS2 (`nros-board-rtic-mps2-an385`) that
  reuses the existing `nros-board-mps2-an385` LAN9118/semihosting bringup,
  implements `RticBoardEntry`, and exposes the same deferred SPSC runtime
  shape as the STM32F4 RTIC board.
* Make the `nros::main!()` RTIC branch board-descriptor driven instead of
  STM32F4-literal driven. The macro must vary the RTIC `device = ...`,
  `dispatchers = [...]`, and dispatch-consumer accessor per deploy key.
* Add a bounded QEMU RTIC Entry fixture that uses `nros::main!()` and
  `node_pkgs = [...]`, then exits through semihosting after proving a
  generated Node callback fired through `Executor::dispatch_callback`.
* Keep the explicit `node_pkgs` -> `<pkg>::register_dispatch(&mut executor)`
  path as the v1 registration mechanism. The older linkme/section-walk idea
  is retired for 216.B because embedded linker-section behavior is harder to
  validate and unnecessary once Entry metadata names the Node packages.

**Priority.** P1 — bare-metal framework support is in tree (RTIC + Embassy
+ stm32f4 examples) but uses Pattern A escape-hatch (`Executor::open` +
hand-written `spin_once` loops) instead of the Phase 212.N.9 `nros::main!()`
shape. Closes the UX gap.

**Depends on.** Phase 212.N.1-N.12 (Board trait family + `nros::main!()`
proc-macro + Component → Node rename) + Phase 212.M-F.13 (macro re-export
via `nros::__macro_support`). Standing on the substrate Phase 212 froze.

**Design doc cross-refs.** `docs/design/0024-multi-node-workspace-layout.md` §11
(3-pkg-role lock; §11.8 escape hatch); `book/src/internals/rmw-backends.md`
(executor + `NodeDispatchRuntime` contract — pre-214.K.1 this was named
`ComponentRuntime`; rename verified in `packages/core/nros-platform/src/
board/runtime.rs:79`).

## Trait surface after 212.N.12 + 214.K.1

The pre-existing 216 design assumed a single `ComponentRuntime` trait
carrying `on_callback`. The current tree splits the surface across
three traits:

| Trait                          | Crate / file                                      | Role                                                  |
|--------------------------------|---------------------------------------------------|-------------------------------------------------------|
| `nros::Node`                   | `packages/core/nros/src/node.rs:69`               | Declarative — `NAME` + `register(ctx)`               |
| `nros::ExecutableNode`         | `packages/core/nros/src/node.rs:1157`             | Runtime — `init() -> Self::State`, `on_callback(state, cb, ctx)`, `tick(state, ctx)` |
| `nros::NodeRuntime`            | `packages/core/nros/src/node.rs:112`              | User-facing sink — `create_node` / `create_entity` / `record_callback_effect` |
| `nros_platform::NodeDispatchRuntime` | `packages/core/nros-platform/src/board/runtime.rs:79` | Board-side dispatch sink (post-214.K.1 rename from `NodeRuntime`) |

Phase 216 lands its new methods accordingly:

* `DISPATCH` const → on `Node` (declarative; visible at codegen time).
* `signal_callback(cb_id, ctx)` → on `NodeDispatchRuntime` (the
  board-side dispatch sink; Deferred runtimes enqueue, Inline forwards
  inline, FromIsr panics on non-ISR-safe contexts).
* `dispatch_strategy() -> DispatchStrategy` → on `NodeDispatchRuntime`
  (board declares which strategy it can serve).
* `on_callback` already lives on `ExecutableNode` — Phase 216 does NOT
  add a duplicate; tag-based dispatch reuses the existing signature
  `fn on_callback(state: &mut Self::State, callback: CallbackId<'_>,
  ctx: &mut CallbackCtx<'_>)`. `Self::State` carries the tag fields the
  Component author matches on.

Every "ComponentRuntime" reference below is post-rename
`NodeDispatchRuntime`. Every "Component pkg" reference at the API/trait
layer is post-N.12 "Node pkg" (the 3-pkg-role taxonomy
Application/Node/Bringup keeps "Component pkg" alive at the workspace
layout layer; Phase 216 spec uses "Node pkg" when talking about traits
+ callbacks).

## Overview — three patterns, two integrations

Phase 212's `nros::main!()` proc-macro expands to
`<Board as BoardEntry>::run(|runtime| run_plan(runtime))`. The board owns
the spin loop; user code is one line.

Three frameworks own their own main + scheduling:

* **RTIC** — `#[rtic::app]` macro generates `fn main`; tasks fire on
  hardware interrupts; nros must run as RTIC tasks.
* **Embassy** — `#[embassy_executor::main]` async fn; tasks fire via async
  polling; nros must run as Embassy tasks.
* **(future) ISR-driven custom** — callbacks fire from interrupt handler
  context; design slot reserved (`DispatchStrategy::FromIsr`), impl
  deferred.

Current bare-metal examples (`examples/stm32f4/rust/*-rtic`,
`*-embassy`) hand-write the integration: `Executor::open` in `#[init]`,
`spin_once(0)` in a low-priority task. Works for pub-only Components.
Cannot dispatch subscriber/service/action callbacks under the framework
without a deferred-dispatch contract — which Phase 216 introduces.

## Architecture

Two-track composition:

### Track A — Substrate

`DispatchStrategy` enum in `nros-platform`. Node pkgs declare a
strategy via `Node::DISPATCH` (defaulted to `Inline` — preserves every
existing Node pkg unchanged). `NodeDispatchRuntime` (the board-side
dispatch sink, post-214.K.1 name) gains `signal_callback` (default
panics) + `dispatch_strategy` query (default `Inline`). `nros::node!()`
macro emits an extra ABI symbol per pkg exposing the strategy + an
`on_callback` trampoline that calls into the existing `ExecutableNode::
on_callback(state, cb, ctx)` shape.

Tag-based callback API (`create_subscription_static`,
`create_service_static`, `create_action_static`) for Deferred Nodes.
Existing closure-based API stays for Inline. Macro lint rejects mixed use.

### Track B — RTIC integration

`nros-board-rtic-<chip>` family. Each crate provides:

* A `Pac` type alias (the chip's PAC crate),
* A `DISPATCHERS: &'static [&'static str]` const (RTIC dispatcher list,
  e.g. `&["USART1", "USART2"]`),
* `RticRuntime: NodeDispatchRuntime` with `DispatchStrategy::Deferred` —
  signaled callbacks land in a `heapless::spsc::Queue`,
* An `init_hardware(cx) -> (Executor, RticRuntime)` fn the macro calls
  from inside the generated `#[init]` body,
* A `RticBoardEntry` trait sibling to `BoardEntry` (the `Owned` variant
  for board-owns-spin boards stays at `BoardEntry`; framework-owned shape
  is the new trait).

`nros::main!()` proc-macro inspects the Entry pkg's deploy target,
looks up a small RTIC board descriptor (board ZST path, PAC module path,
dispatcher interrupts, dispatch-consumer accessor), and emits a
`#[rtic::app(...)]` module. The first landed body collapsed the earlier
two-task sketch into one `__nros_run` software task because RTIC local
resources are claimed exclusively and both `spin_once` and deferred
callback dispatch need mutable access to the same executor. The task:

1. calls `executor.spin_once(Duration::from_millis(1))`,
2. drains the board runtime's SPSC consumer, and
3. forwards every `(cb_id, ctx_ptr)` into `Executor::dispatch_callback`.

Node packages are registered explicitly from Entry metadata:
`[package.metadata.nros.entry] node_pkgs = ["pkg_a", ...]`. For each entry,
the macro emits `<pkg>::register_dispatch(&mut executor)`, and the
`nros::node!()` expansion pushes `(state_ptr,
__nros_node_<pkg>_on_callback)` into the executor's dispatch-slot table.
This replaces the pre-refresh linkme/section-walk idea for 216.B.

User custom RTIC tasks via `nros::main!(custom_tasks = [my_adc, my_ui])`
syntax — proc-macro folds extra task fns into the generated module.

### Track C — Embassy integration

`nros-board-embassy-<chip>` family. Mirrors RTIC with Embassy primitives:

* `embassy_sync::channel::Channel<...>` instead of SPSC queue,
* `EmbassyBoardEntry` trait (sibling to `RticBoardEntry`),
* `nros::main!()` expands to `#[embassy_executor::main] async fn main(...)`
  with `__nros_spin_task` + `__nros_dispatch_task` spawned via Spawner.

### Sync-only `on_callback` (no AsyncNode for v1)

Embassy users that need async work downstream from a callback spawn an
Embassy task from inside their sync `on_callback`:

```rust
fn on_callback(&mut self, _cb_id: CallbackId, ctx: CallbackCtx) {
    let msg: Int32 = ctx.downcast().unwrap();
    self.spawner.spawn(handle_msg(msg)).unwrap();
}

#[embassy_executor::task]
async fn handle_msg(msg: Int32) {
    spi.write(&msg.data.to_le_bytes()).await.unwrap();
}
```

Preserves the bare-metal no-alloc contract (no `Box<dyn Future>`, no
GAT machinery). `AsyncNode` trait via RPITIT deferred to Phase 216.D
follow-up; revisit only if real usage shows the spawn-from-sync pattern
is too painful.

### Tag-based vs closure callback API

Closure-based subscription registration (`ctx.create_subscription("topic",
|msg| {...})`) captures state by value — closure types are unknown to the
runtime; storing them generically in a no-alloc context requires either
type erasure (boxing → alloc) or storing as `extern "C" fn` pointers
(loses captures).

Tag-based registration (Phase 216.A.4): Node author declares a
`CallbackTag` per registration, holds the tag on the Node's `State`
struct (Node + ExecutableNode are split traits — state is owned by
the generated runtime + threaded through `on_callback`'s first arg),
dispatches in `on_callback` via tag match. No alloc, no boxing.

```rust
pub struct Listener;
pub struct ListenerState {
    sub_chatter: SubscriptionTag,
}
impl Node for Listener {
    const NAME: &'static str = "listener";
    const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;
    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        // For Deferred Components, registration uses the `_static`
        // variants; the SubscriptionTag is recorded into the generated
        // State via the macro-emitted glue (see 216.A.5).
        let _tag = ctx.create_subscription_static::<Int32>("/chatter")?;
        Ok(())
    }
}
impl ExecutableNode for Listener {
    type State = ListenerState;
    fn init() -> Self::State {
        // Tags resolved by the macro-emitted init body (see 216.A.5);
        // shown explicit here for clarity.
        ListenerState { sub_chatter: SubscriptionTag::placeholder() }
    }
    fn on_callback(state: &mut Self::State, cb: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
        if cb == state.sub_chatter.into() {
            let msg: Int32 = ctx.downcast().unwrap();
            defmt::info!("Received: {}", msg.data);
        }
    }
}
nros::node!(Listener);
```

Inline Nodes keep the closure API (no migration cost). Macro lint
forbids Deferred Nodes from using closure-based registration.

## Work Items

### 216.A — Substrate (foundation; backward-compat-preserving)

- [x] **216.A.1** — `DispatchStrategy` enum in
      `packages/core/nros-platform/src/runtime.rs`:
      ```rust
      #[derive(Copy, Clone, Debug, PartialEq, Eq)]
      pub enum DispatchStrategy {
          Inline,
          Deferred,
          FromIsr,   // design slot only; impl deferred to Phase 216.E
      }
      ```
      `#[repr(u8)]` for FFI stability. Re-export through
      `nros::DispatchStrategy`.
      **Files**: `packages/core/nros-platform/src/runtime.rs`,
      `packages/core/nros/src/lib.rs` (re-export).
      **Landed:** `09220cef7`

- [x] **216.A.2** — `NodeDispatchRuntime` trait extensions (post-214.K.1
      name; lives at `packages/core/nros-platform/src/board/runtime.rs`,
      NOT `src/runtime.rs`):
      ```rust
      pub trait NodeDispatchRuntime {
          // ... existing methods unchanged

          fn signal_callback(&mut self, _cb_id: CallbackId<'_>, _ctx: &mut CallbackCtx<'_>) {
              panic!("signal_callback not implemented for Inline runtime");
          }
          fn dispatch_strategy(&self) -> DispatchStrategy {
              DispatchStrategy::Inline
          }
      }
      ```
      Defaulted methods → zero-touch for existing impls
      (`ExecutorNodeRuntime` in `nros`, `NullNodeRuntime` in
      `nros-platform`); `Inline` runtime keeps working unchanged.
      **Files**: `packages/core/nros-platform/src/board/runtime.rs`.
      **Landed:** `8994163df` (adds `signal_callback` +
      `dispatch_strategy` + `SignaledCallback` newtype).

- [x] **216.A.3** — `Node` trait extension (declarative side):
      ```rust
      pub trait Node {
          const NAME: &'static str;
          /// Phase 216.A.3 — declares which dispatch strategy this Node
          /// requires from the runtime. `Inline` (default) is served by
          /// every runtime; `Deferred` requires a framework-aware board
          /// (RTIC/Embassy); `FromIsr` design slot, not yet impl'd.
          const DISPATCH: DispatchStrategy = DispatchStrategy::Inline;

          fn register(context: &mut NodeContext<'_>) -> NodeResult<()>;
      }
      ```
      Note: `on_callback` already lives on `ExecutableNode` (separate
      trait, `node.rs:1157`); the existing signature
      `fn on_callback(state: &mut Self::State, callback: CallbackId<'_>,
      ctx: &mut CallbackCtx<'_>)` is preserved. Phase 216 does NOT
      change it — the Deferred path reuses the same trampoline; the
      runtime calls `on_callback` from its dispatch task instead of the
      spin task. Defaulted associated `const` is stable on edition 2024.
      **Files**: `packages/core/nros/src/node.rs` (the `Node` trait).
      **Landed:** `429583987`

- [x] **216.A.4** — Tag-based callback API. New `_static` registration
      variants:
      ```rust
      impl<'a> NodeContext<'a> {
          pub fn create_subscription_static<M: Message>(
              &mut self,
              topic: &'static str,
          ) -> NodeResult<SubscriptionTag>;

          pub fn create_service_static<S: Service>(
              &mut self,
              name: &'static str,
          ) -> NodeResult<ServiceTag>;

          pub fn create_action_static<A: Action>(
              &mut self,
              name: &'static str,
          ) -> NodeResult<ActionTag>;
      }
      ```
      `*Tag` types wrap a `CallbackId`; `From<*Tag> for CallbackId` impls
      let `on_callback` match on tag. Tag-based registrations route through
      `signal_callback` on Deferred runtimes; on Inline runtimes they
      route through the inline dispatch path with a default-impl `on_callback`
      stub.
      **Files**: `packages/core/nros-node/src/node_context.rs`,
      `packages/core/nros/src/callback.rs` (Tag types).
      **Landed:** `6bb7975c6` (SubscriptionTag / ServiceTag /
      ActionTag types) + `1fbf0f5d9` (followup:
      `DeclaredNode::create_{subscription,service,action}_static`
      ctors).

- [x] **216.A.5** — `nros::node!()` macro extensions:
      ```rust
      // EXPANDS TO (additions to current emit):
      #[unsafe(no_mangle)]
      extern "C" fn __nros_node_<pkg>_dispatch_strategy() -> u8 {
          <<Type> as ::nros::Node>::DISPATCH as u8
      }

      #[unsafe(no_mangle)]
      extern "C" fn __nros_node_<pkg>_on_callback(
          state: *mut core::ffi::c_void,
          cb_id: ::nros::CallbackId,
          ctx: ::nros::CallbackCtx,
      ) {
          let state = unsafe { &mut *(state as *mut <Type>) };
          state.on_callback(cb_id, ctx);
      }
      ```
      Symbols emit via `::nros::__macro_support::nros_platform::*` path
      per M-F.13 contract.
      **Files**: `packages/core/nros-macros/src/lib.rs` (the
      `nros::node!()` macro body).
      **Landed:** `33d46aca7`

- [ ] **216.A.6** — Lint: macro rejects Deferred Components using closure
      registration. Detection during macro expansion: if `<T>::DISPATCH ==
      Deferred` and `register` body contains `create_subscription(.., |..|)`
      (closure-arg variant), compile error. Similar for service/action.
      Spans + diagnostics point at the offending registration.
      **Files**: `packages/core/nros-macros/src/lib.rs` (lint emission).
      **Deferred:** speculative — no concrete misuse pattern has
      surfaced. Substrate ABI is in place to add this lint later
      without breaking changes; revisit if a real user trips on the
      missing rejection.

- **Tests** (file: `packages/testing/nros-tests/tests/phase216_a_dispatch_strategy.rs`):
  - [ ] `dispatch_strategy_default_is_inline` — `Node` trait default
        gives `Inline`.
  - [ ] `inline_node_dispatches_via_closure` — existing Inline pattern
        keeps working post-substrate; every Phase 212 Node pkg under
        `examples/` compiles unchanged.
  - [ ] `deferred_node_dispatches_via_on_callback` — POSIX-side smoke
        with a synthetic Deferred `NodeDispatchRuntime` exercises
        `signal_callback` + `ExecutableNode::on_callback`.
  - [ ] `lint_rejects_closure_in_deferred_node` — macro emits a clear
        compile error.

### 216.B — RTIC integration

- [x] **216.B.1** — `RticBoardEntry` trait sibling to `BoardEntry`
      (framework-owned-spin shape):
      ```rust
      pub trait RticBoardEntry: Board {
          type Pac: 'static;
          const DISPATCHERS: &'static [&'static str];

          /// Called from inside the proc-macro-generated `#[init]` body.
          /// Returns the Executor + framework-aware NodeDispatchRuntime
          /// the proc-macro wires into RTIC `#[local]` storage.
          fn init_hardware(
              device: Self::Pac,
              core: cortex_m::Peripherals,
          ) -> (Executor, Self::Runtime);

          type Runtime: NodeDispatchRuntime;
      }
      ```
      Distinct from `BoardEntry` (which keeps the board-owns-spin
      contract for POSIX + RTOS boards).
      **Files**: `packages/core/nros-platform/src/board.rs`.
      **Landed:** `2a4df2c93`

- [~] **216.B.2** — `nros-board-rtic-stm32f4` crate:
      `packages/boards/nros-board-rtic-stm32f4/`. Provides:
      * `Pac = stm32f4xx_hal::pac` (chip-specific PAC),
      * `RticStm32F4: Board + BoardInit + RticBoardEntry`,
      * `RticRuntime: NodeDispatchRuntime` w/ `DispatchStrategy::
        Deferred` + `signal_callback` via `heapless::spsc::Producer`,
      * Static SPSC queue declared via `nros_rtic_runtime!` macro from
        `nros-board-rtic-common` (companion crate for shared queue
        machinery + dispatch routing).
      `[package.metadata.nros.board] framework = "rtic"` so
      `nros::main!()` proc-macro discovers the framework kind.
      **Files**: `packages/boards/nros-board-rtic-stm32f4/`,
      `packages/boards/nros-board-rtic-common/` (shared queue +
      dispatch macros).
      **Landed (skeleton):** `ab5fd5e9d` (crate skeleton +
      `RticBoardEntry` impl) + `b5b371d09` (followup: `RticRuntime`
      SPSC + `signal_callback` impl).
      **Landed (follow-up):** `init_hardware` now delegates to the
      direct-exec `nros-board-stm32f4` bringup, registers the zenoh RMW
      explicitly, opens `nros::Executor`, and splits the deferred SPSC
      queue. Remaining STM32F4 work is portability validation only; it is
      no longer the E2E gate.

- [ ] **216.B.2.qemu** — `nros-board-rtic-mps2-an385` emulator board:
      `packages/boards/nros-board-rtic-mps2-an385/`. Provides:
      * `Pac = mps2_an385_pac::Peripherals`,
      * `RticMps2An385: Board + RticBoardEntry`,
      * `RticRuntime: NodeDispatchRuntime` with `DispatchStrategy::Deferred`
        and SPSC-backed `signal_callback`,
      * QEMU/slirp-friendly hardware init that delegates to
        `nros_board_mps2_an385::init_hardware(&Config)`, registers zenoh
        RMW, opens `nros::Executor`, and stashes the SPSC consumer for
        `__nros_run`.
      `[package.metadata.nros.board] framework = "rtic"` so `nros check`
      can validate `(rtic, deferred)` workspaces.

- [~] **216.B.3** — `nros::main!()` proc-macro RTIC routing branch:
      ```rust
      // For deploy = "rtic-stm32f4":
      #[rtic::app(device = ::nros_board_rtic_stm32f4::pac,
                  dispatchers = [USART1, USART2])]
      mod __nros_app {
          use super::*;
          use ::nros::*;
          use ::nros_board_rtic_stm32f4::*;

          #[shared] struct Shared {}
          #[local] struct Local {
              executor: Executor,
              runtime: RticRuntime,
              // <one entry per registered Component, holding its state>
          }

          #[init]
          fn init(cx: init::Context) -> (Shared, Local) {
              let (mut executor, mut runtime) =
                  RticStm32F4::init_hardware(cx.device, cx.core);
              run_plan(&mut runtime).expect("run_plan");
              __nros_spin::spawn().unwrap();
              __nros_dispatch::spawn().unwrap();
              (Shared {}, Local { executor, runtime, /* states */ })
          }

          #[task(local = [executor], priority = 1)]
          async fn __nros_spin(cx: __nros_spin::Context) { /* spin loop */ }

          #[task(local = [runtime, /* states */], priority = 2)]
          async fn __nros_dispatch(cx: __nros_dispatch::Context) {
              while let Some((cb_id, ctx)) = cx.local.runtime.dequeue() {
                  // route via per-pkg `__nros_node_<pkg>_on_callback`
                  ::nros::dispatch_to_node(cb_id, ctx, cx.local);
              }
          }
      }

      fn main() -> ! { unreachable!("RTIC owns main"); }
      ```
      Proc-macro reads board metadata (`framework = "rtic"`) +
      enumerates registered Nodes from `run_plan`'s symbol table to
      emit per-Node `#[local]` entries + dispatch routing.
      **Files**: `packages/core/nros-macros/src/main_macro.rs`.
      **Landed (skeleton):** `d8ce91226` (RTIC routing branch in
      `nros::main!()`) + `b8e5f76f8` (followup: spawn `__nros_spin`
      + `__nros_dispatch` tasks).
      **Landed (follow-up):** the branch now registers explicit
      `node_pkgs`, spawns a collapsed `__nros_run` task, drives
      `executor.spin_once`, drains the board SPSC consumer, and forwards
      callbacks to `Executor::dispatch_callback`.
      **Remaining:** make RTIC board emit descriptor-driven so the same
      branch supports both `rtic-stm32f4` and the emulator deploy key
      `rtic-mps2-an385`.

- [x] **216.B.4** — `nros::main!(custom_tasks = [my_adc, my_ui])`
      syntax. Proc-macro folds extra `#[rtic_task]`-annotated fns into
      the generated `mod __nros_app` body. Token-tree extraction;
      preserve user fn signatures verbatim.
      **Files**: `packages/core/nros-macros/src/main_macro.rs`.
      **Landed:** `a2046487d`

- [~] **216.B.5** — Migrate the six existing stm32f4 RTIC examples to
      `nros::main!()` shape. Current pattern is Pattern A escape-hatch
      (`Executor::open` + hand-written `net_poll` task w/ `spin_once(0)`
      — verified at `examples/stm32f4/rust/talker-rtic/src/main.rs:71,93`).
      Targets (all already exist in tree):
      * `examples/stm32f4/rust/talker-rtic/` — pub-only, `DISPATCH =
        Inline`. Collapse `src/main.rs` to `nros::main!();`. Swap
        Cargo.toml dep `nros-board-stm32f4` → `nros-board-rtic-stm32f4`.
        Add `[package.metadata.nros.entry] deploy = "rtic-stm32f4"`.
        Carve the publisher body into a Node pkg `talker_pkg/` with
        `impl Node for Talker` + `nros::node!(Talker)`.
      * `examples/stm32f4/rust/listener-rtic/` — sub-driven; the
        canonical Deferred exemplar. `DISPATCH = Deferred`, tag-based
        subscription. `defmt::info!` from `on_callback` proves the
        Deferred dispatch path fires from the `__nros_dispatch` task
        (not the spin task).
      * `examples/stm32f4/rust/service-server-rtic/` — `DISPATCH =
        Deferred`, tag-based `create_service_static`.
      * `examples/stm32f4/rust/service-client-rtic/` — request side;
        Inline if no callbacks, Deferred if it sub'd to a response
        topic.
      * `examples/stm32f4/rust/action-server-rtic/` — `DISPATCH =
        Deferred`, tag-based `create_action_static`. Exercises the
        feedback / result paths.
      * `examples/stm32f4/rust/action-client-rtic/` — sibling client.
      **Files**: `examples/stm32f4/rust/{talker,listener,service-server,
      service-client,action-server,action-client}-rtic/{src/main.rs,
      Cargo.toml}` + 6 new sibling Node pkgs `examples/stm32f4/rust/
      {talker,listener,service_server,service_client,action_server,
      action_client}_pkg/`.
      **Landed (skeleton):** all six examples migrated to
      `nros::main!()` shape with carved `*_pkg/` Node packages —
      `a7620ab43` (talker-rtic), `0e42ebaf0` (listener-rtic),
      `9bd703dfe` (service-server-rtic), `5c9dc0dba`
      (service-client-rtic), `4c97f6efe` (action-server-rtic),
      `76a4791c4` (action-client-rtic).
      **Remaining:** examples compile + boot but inherit the
      placeholder dispatch from 216.B.3 — Deferred variants
      (listener, service-server, action-server) won't actually fire
      `on_callback` from the dispatch task until the routing body
      lands.

- [ ] **216.B.6** — Coverage gate: at least one Node pkg per dispatch
      strategy variant (Inline + Deferred) shipping under
      `examples/stm32f4/rust/`. `talker-rtic` covers Inline,
      `listener-rtic` covers Deferred. (Was: "Add listener-rtic" — the
      example already exists; the gate is now "every variant exercised
      end-to-end on real or QEMU hardware".)
      **Files**: same set as 216.B.5.

- [ ] **216.B.7** — Emulator E2E gate. Add a QEMU MPS2-AN385 RTIC Entry
      fixture using the real 216.B UX:
      ```rust
      #![no_std]
      #![no_main]
      use panic_semihosting as _;
      nros::main!();
      ```
      with `[package.metadata.nros.entry] deploy = "rtic-mps2-an385"` and
      `node_pkgs = [...]`. The fixture must be bounded and semihosting-exit
      with success after a generated Deferred Node's `on_callback` fires
      through the RTIC SPSC queue and `Executor::dispatch_callback`.
      STM32F4 remains compile-only coverage; QEMU is the required E2E
      validation path.

- **Tests:**
  - [ ] `phase216_b_rtic_main_macro_expansion` — UI test asserts
        `nros::main!()` for an `rtic-stm32f4` deploy target expands to
        the expected `#[rtic::app]` skeleton.
  - [ ] `phase216_b_rtic_callback_dispatch_e2e` — QEMU MPS2-AN385
        generated Entry fixture. A Deferred Node callback is signaled,
        queued by the RTIC runtime, drained by `__nros_run`, dispatched
        via `Executor::dispatch_callback`, and the firmware exits
        success through semihosting.

### 216.C — Embassy integration

- [x] **216.C.1** — `EmbassyBoardEntry` trait sibling to `RticBoardEntry`:
      ```rust
      pub trait EmbassyBoardEntry: Board {
          type Spawner: 'static;
          const CHANNEL_CAPACITY: usize = 32;

          fn init_hardware(spawner: Spawner) -> (Executor, Self::Runtime);
          type Runtime: NodeDispatchRuntime;
      }
      ```
      **Files**: `packages/core/nros-platform/src/board.rs`.
      **Landed:** `9de4b227e`

- [~] **216.C.2** — `nros-board-embassy-stm32f4` crate:
      `packages/boards/nros-board-embassy-stm32f4/`. `EmbassyRuntime`
      uses `embassy_sync::channel::Channel<NoopRawMutex, (CallbackId,
      CallbackCtx), CHANNEL_CAPACITY>` instead of SPSC queue. Channel
      is static; `try_send` from `signal_callback` (non-blocking; drops
      on full + emits log warning).
      `[package.metadata.nros.board] framework = "embassy"`.
      **Files**: `packages/boards/nros-board-embassy-stm32f4/`,
      `packages/boards/nros-board-embassy-common/`.
      **Landed (skeleton):** `fc4213c4e` (crate skeleton +
      `EmbassyBoardEntry` impl) + `d7cbd8148` (followup:
      `EmbassyRuntime` channel + `signal_callback` impl).
      **Remaining:** `init_hardware` clock + peripheral bringup body
      is still a placeholder pending the real stm32f4 Embassy HAL
      wiring.

- [~] **216.C.3** — `nros::main!()` proc-macro Embassy routing branch:
      ```rust
      // For deploy = "embassy-stm32f4":
      use ::nros_board_embassy_stm32f4::*;

      #[embassy_executor::main]
      async fn main(spawner: ::embassy_executor::Spawner) -> ! {
          let (executor, runtime) = EmbassyStm32F4::init_hardware(spawner).await;
          spawner.spawn(__nros_spin_task(executor)).unwrap();
          spawner.spawn(__nros_dispatch_task(runtime)).unwrap();
          run_plan(&mut runtime).expect("run_plan");
          loop { ::embassy_time::Timer::after_secs(60).await; }
      }

      #[::embassy_executor::task]
      async fn __nros_spin_task(executor: ::nros::Executor) { /* loop */ }

      #[::embassy_executor::task]
      async fn __nros_dispatch_task(runtime: EmbassyRuntime) { /* loop */ }
      ```
      **Files**: `packages/core/nros-macros/src/main_macro.rs`.
      **Landed (skeleton):** `6adb2d202` (Embassy routing branch in
      `nros::main!()`) + `1671f5095` (followup: spawn
      `__nros_spin_task` + `__nros_dispatch_task`).
      **Remaining:** task bodies are placeholders — same
      trampoline-registration story blocker as 216.B.3.

- [x] **216.C.4** — Migrate `examples/stm32f4/rust/talker-embassy/` to
      `nros::main!()` shape (sibling to 216.B.5; the example already
      exists in tree and currently runs Pattern A escape-hatch — verified
      at `examples/stm32f4/rust/talker-embassy/src/main.rs:120` w/
      hand-written `spin_once` loop). Inline `DISPATCH` for the pub-only
      shape.
      **Files**: `examples/stm32f4/rust/talker-embassy/{src/main.rs,
      Cargo.toml}` + carved Node pkg `examples/stm32f4/rust/
      talker_pkg/` (shared with B.5 — the Node pkg is board-agnostic).
      **Landed:** `6794279ba`

- [x] **216.C.5** — Add `examples/stm32f4/rust/listener-embassy/` —
      callback-driven Deferred Node (sibling to 216.B.5's listener-rtic
      migration; no Embassy listener example exists yet, only the RTIC
      variant). Demonstrates the spawn-from-sync escape:
      ```rust
      impl ExecutableNode for Listener {
          type State = ListenerState;
          fn on_callback(state: &mut Self::State, _cb: CallbackId<'_>,
                         ctx: &mut CallbackCtx<'_>) {
              let msg: Int32 = ctx.downcast().unwrap();
              state.spawner.spawn(handle_downstream(msg)).unwrap();
          }
      }
      ```
      The Embassy `Spawner` lives on `Self::State`, populated by the
      generated `init()` hook (216.A.5).
      **Files**: `examples/stm32f4/rust/listener-embassy/` (new),
      `examples/stm32f4/rust/listener_pkg/` (shared with the RTIC
      listener — the Node pkg stays board-agnostic).
      **Landed:** `a3de8b4f8` (also ships the Deferred-dispatch
      `listener_pkg/` carve).

- **Tests:**
  - [ ] `phase216_c_embassy_main_macro_expansion` — UI test.
  - [ ] `phase216_c_embassy_callback_dispatch_e2e` — sibling to
        216.B's e2e.
  - [ ] `phase216_c_embassy_spawn_from_callback` — verifies the
        spawn-from-sync escape pattern compiles + runs (does NOT verify
        the spawned task completes; that's user code).

### 216.D — `nros check` lint + docs

- [ ] **216.D.1** — `nros check` cross-validates Node `DISPATCH`
      against Entry pkg board framework. Logic:
      ```
      for each Node pkg in workspace:
          strategy = read __nros_node_<pkg>_dispatch_strategy() ABI
          for each Entry pkg deploying <Node>:
              framework = lookup board's [package.metadata.nros.board].framework
              require: (framework, strategy) in MATRIX
      ```
      MATRIX:
      | framework | Inline | Deferred | FromIsr |
      |---|---|---|---|
      | posix | ✓ | ✓ | error: ISR not on posix |
      | rtos | ✓ | ✓ | error: ISR not on rtos |
      | rtic | error: prefer Deferred | ✓ | ✓ (future) |
      | embassy | error: prefer Deferred | ✓ | error: ISR not on embassy |

      Mismatch errors include suggested fix (e.g. "add
      `const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;`
      to your `impl Node for Talker`").
      **Files**: `nros-cli/packages/nros-cli-core/src/cmd/check.rs` (in
      the standalone nros-cli repo).
      **Deferred:** lives outside this superproject — the in-tree
      codegen submodule was retired (Phase 195.D) and the `nros` CLI
      now ships prebuilt from `github.com/NEWSLabNTU/nros-cli`.
      Substrate ABI (`__nros_node_<pkg>_dispatch_strategy` from
      216.A.5) is in place so the lint has a hook to read from when
      it lands in nros-cli.

- [x] **216.D.2** — Book chapters:
      * `book/src/user-guide/rtic-integration.md` — RTIC tutorial,
        walking from `nros::main!()` to custom-task escape, with
        the Node pkg + Entry pkg shape (3-pkg-role taxonomy per
        `0024-multi-node-workspace-layout.md` §11).
      * `book/src/user-guide/embassy-integration.md` — mirror for
        Embassy, covering the spawn-from-sync escape.
      * `book/src/internals/dispatch-strategy.md` — design rationale
        for the Inline / Deferred / FromIsr trichotomy + the tag-based
        callback API.
      **Files**: `book/src/{user-guide,internals}/*.md` (3 new pages) +
      `book/src/SUMMARY.md`.
      **Landed:** `3b5435e1e`

### 216.E — Future work (deferred, design slots only)

- [ ] **216.E.1** — `DispatchStrategy::FromIsr` impl. Requires:
      * Reentrancy audit of the spin-loop dispatch path
      * Lock-free SPSC variant tolerant of ISR-priority producer
      * Per-Component `#[isr_safe]` proof contract
      Land when a real ISR-driven driver demands it (e.g. timer pulse
      → `nros::node!()` Node dispatched directly from the timer ISR
      with no scheduler hop).

- [ ] **216.E.2** — `AsyncNode` trait via RPITIT. Only land if
      spawn-from-sync (216.C.5 pattern) is consistently painful in
      real Embassy usage. Shape:
      ```rust
      pub trait AsyncNode: 'static {
          const NAME: &'static str;
          const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;

          fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()>;
          async fn on_callback(
              &mut self,
              cb_id: CallbackId,
              ctx: CallbackCtx,
          );
      }
      ```
      Compiles on Embassy targets; rejected on RTIC + RTOS + POSIX (no
      async runtime to drive). Macro emits a separate `__nros_node_<pkg>_
      on_callback_async` ABI symbol.

- [ ] **216.E.3** — Per-callback strategy (vs per-Component). If real
      usage shows mixed-strategy Components (e.g. one subscription
      Inline + another Deferred), revisit. Likely YAGNI for v1.

## Acceptance

- [x] **Substrate**: `nros::DispatchStrategy` enum exists + `Node`
      trait carries `DISPATCH` const with `Inline` default + every
      existing Node pkg compiles + tests stay green (substrate is
      additive, not breaking). Met by 216.A.1–A.5.
- [~] **RTIC**: all six existing `examples/stm32f4/rust/*-rtic/` mains
      collapse to `nros::main!();`. talker-rtic + listener-rtic build
      + run end-to-end on real or QEMU hardware; listener exercises
      Deferred dispatch with a real subscription; callback fires from
      the `__nros_dispatch` task (not spin task).
      **Met (shape):** all six mains migrated (216.B.5).
      **Gated:** real-or-QEMU end-to-end run — depends on the
      placeholder dispatch-task bodies (216.B.3 Remaining) being
      replaced with the per-Node trampoline routing, plus stm32f4
      bringup (216.B.2 Remaining).
- [~] **Embassy**: `talker-embassy` migrated + new `listener-embassy`
      lands w/ spawn-from-sync escape.
      **Met (shape):** 216.C.4 + 216.C.5.
      **Gated:** same trampoline-routing blocker as RTIC.
- [ ] **Lint**: `nros check` rejects a Node pkg with `DISPATCH = Inline`
      deployed to a `framework = "rtic"` board with a clear error +
      suggested fix.
      **Deferred:** lives in standalone `nros-cli` repo (216.D.1).
- [x] **Books**: three new chapters (RTIC, Embassy, dispatch strategy).
      Met by 216.D.2 (`3b5435e1e`).
- [ ] **Test infra**: `phase216_a_dispatch_strategy.rs` +
      `phase216_b_rtic_*.rs` (×2) + `phase216_c_embassy_*.rs` (×3) all
      pass on a CI lane with QEMU + thumbv7m + Embassy/RTIC toolchains.
      Test files not yet authored; gated on the dispatch-task wiring
      landing.
- [x] **Backward compat**: every Phase 212 Node pkg keeps working
      without code changes (Inline default + closure API stay
      operational). Met by the defaulted `DISPATCH = Inline` const
      (216.A.3) + the additive `NodeDispatchRuntime` extensions
      (216.A.2 defaulted methods).

## Notes / cross-refs

* The escape hatch (Pattern A, `Executor::open` + hand-written
  `spin_once`) stays documented + supported as the "I want full control"
  path. Phase 216 adds the ergonomic path on top, not replaces.
* `BoardEntry` (POSIX/RTOS board-owns-spin) + `RticBoardEntry` +
  `EmbassyBoardEntry` are three sibling traits. `Board` is the shared
  base (init, transport, exit) — only the *spin model* differs.
* Tag-based registration coexists with closure-based: Inline keeps
  closures (zero migration cost); Deferred uses tags. Macro lint
  enforces the split.
* Sync-only `on_callback` for v1. Spawn-from-sync (Embassy `Spawner` or
  RTIC `spawn::Foo`) is the official escape for downstream async work.
  `AsyncNode` (216.E.2) lands only if real usage proves spawn-from-sync
  is too painful.
* `DispatchStrategy::FromIsr` is a design slot; impl deferred (216.E.1)
  until a real ISR-driven Node lands.
* Phase 215 (Board crate as importable unit, FVP-first) is orthogonal:
  215 lands the cmake/west import surface for external Zephyr
  consumers; 216 lands the runtime dispatch story for bare-metal
  framework users. They share the `[package.metadata.nros.board]`
  schema (Phase 215.C.1 ↔ 216.B.2/C.2 `framework = "..."` field) — the
  metadata loader should converge so both phases agree on the parsed
  shape. Otherwise independent; either can ship first.
* Cross-references:
  * `docs/design/0024-multi-node-workspace-layout.md` §11.8 (escape hatch)
  * `docs/roadmap/phase-212-ux-cargo-native-and-file-consolidation.md`
    §N (Board trait family + N.9 proc-macro)
  * `docs/roadmap/phase-213-post-212-known-issues.md` §B (cmake fn
    rename — analogous to Phase 216's user-facing-API stability work)
  * `book/src/internals/rmw-backends.md`
  * `book/src/getting-started/integration-stm32f4.md` (existing RTIC
    + Embassy starter; will cross-link Phase 216 chapters)
