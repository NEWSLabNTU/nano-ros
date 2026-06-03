# Phase 216 — Bare-metal Framework Integration (RTIC + Embassy)

**Goal.** `nros::main!()` works cleanly on RTIC + Embassy bare-metal targets
with the same one-line UX as POSIX + RTOS. Component pkgs declare a
`DispatchStrategy` and stay framework-portable; callbacks fire from the
framework's task scheduler (not nros's spin loop) when running on a
framework-aware board.

**Status.** Design locked 2026-06-03 (B+C composition; sync-only callbacks
for v1; tag-based registration API for Deferred Components).

**Priority.** P1 — bare-metal framework support is in tree (RTIC + Embassy
+ stm32f4 examples) but uses Pattern A escape-hatch (`Executor::open` +
hand-written `spin_once` loops) instead of the Phase 212.N.9 `nros::main!()`
shape. Closes the UX gap.

**Depends on.** Phase 212.N.1-N.12 (Board trait family + `nros::main!()`
proc-macro + Component → Node rename) + Phase 212.M-F.13 (macro re-export
via `nros::__macro_support`). Standing on the substrate Phase 212 froze.

**Design doc cross-refs.** `docs/design/multi-node-workspace-layout.md` §11
(3-pkg-role lock; §11.8 escape hatch); `book/src/internals/rmw-backends.md`
(executor + ComponentRuntime contract).

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

`DispatchStrategy` enum in `nros-platform`. Component pkgs declare a
strategy via `Node::DISPATCH` (defaulted to `Inline` — preserves every
existing Component pkg unchanged). `ComponentRuntime` trait gains
`signal_callback` (default panics) + `dispatch_strategy` query (default
`Inline`). `nros::node!()` macro emits an extra ABI symbol per pkg
exposing the strategy + an `on_callback` trampoline.

Tag-based callback API (`create_subscription_static`,
`create_service_static`, `create_action_static`) for Deferred Components.
Existing closure-based API stays for Inline. Macro lint rejects mixed use.

### Track B — RTIC integration

`nros-board-rtic-<chip>` family. Each crate provides:

* A `Pac` type alias (the chip's PAC crate),
* A `DISPATCHERS: &'static [&'static str]` const (RTIC dispatcher list,
  e.g. `&["USART1", "USART2"]`),
* `RticRuntime: ComponentRuntime` with `DispatchStrategy::Deferred` —
  signaled callbacks land in a `heapless::spsc::Queue`,
* An `init_hardware(cx) -> (Executor, RticRuntime)` fn the macro calls
  from inside the generated `#[init]` body,
* A `RticBoardEntry` trait sibling to `BoardEntry` (the `Owned` variant
  for board-owns-spin boards stays at `BoardEntry`; framework-owned shape
  is the new trait).

`nros::main!()` proc-macro inspects the Entry pkg's deploy target,
discovers the board crate's framework metadata
(`[package.metadata.nros.board] framework = "rtic"`), and emits a
`#[rtic::app(...)]` module with two auto-generated tasks: `__nros_spin`
(low priority; calls `executor.spin_once(0)` + monotonic yield) and
`__nros_dispatch` (drains the SPSC queue + routes signaled callbacks to
the right Component's `on_callback` via the per-pkg FFI trampoline).

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

Tag-based registration (Phase 216.A.4): Component author declares a
`CallbackTag` per registration, holds state on `&mut self`, dispatches
in `on_callback` via tag match. No alloc, no boxing, state lives on the
struct.

```rust
pub struct Listener {
    sub_chatter: SubscriptionTag,
}
impl Node for Listener {
    const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;
    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        self.sub_chatter = ctx.create_subscription_static::<Int32>("/chatter")?;
        Ok(())
    }
    fn on_callback(&mut self, cb_id: CallbackId, ctx: CallbackCtx) {
        if cb_id == self.sub_chatter.into() {
            let msg: Int32 = ctx.downcast().unwrap();
            defmt::info!("Received: {}", msg.data);
        }
    }
}
nros::node!(Listener);
```

Inline Components keep the closure API (no migration cost). Macro lint
forbids Deferred Components from using closure-based registration.

## Work Items

### 216.A — Substrate (foundation; backward-compat-preserving)

- [ ] **216.A.1** — `DispatchStrategy` enum in
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

- [ ] **216.A.2** — `ComponentRuntime` trait extensions:
      ```rust
      pub trait ComponentRuntime {
          // ... existing methods unchanged

          fn signal_callback(&mut self, _cb_id: CallbackId, _ctx: CallbackCtx) {
              panic!("signal_callback not implemented for Inline runtime");
          }
          fn dispatch_strategy(&self) -> DispatchStrategy {
              DispatchStrategy::Inline
          }
      }
      ```
      Defaulted methods → zero-touch for existing impls; `Inline` runtime
      keeps working unchanged.
      **Files**: `packages/core/nros-platform/src/runtime.rs`.

- [ ] **216.A.3** — `Node` trait extension:
      ```rust
      pub trait Node: 'static {
          const NAME: &'static str;
          const DISPATCH: DispatchStrategy = DispatchStrategy::Inline;

          fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()>;
          fn on_callback(&mut self, _cb_id: CallbackId, _ctx: CallbackCtx) {
              unreachable!("Inline component dispatched via Deferred path");
          }
      }
      ```
      Defaulted associated const (stable in Rust 1.79+; nano-ros uses
      edition 2024, no blocker).
      **Files**: `packages/core/nros/src/component.rs` (or wherever the
      `Node` trait lives post-N.12 rename).

- [ ] **216.A.4** — Tag-based callback API. New `_static` registration
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

- [ ] **216.A.5** — `nros::node!()` macro extensions:
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

- [ ] **216.A.6** — Lint: macro rejects Deferred Components using closure
      registration. Detection during macro expansion: if `<T>::DISPATCH ==
      Deferred` and `register` body contains `create_subscription(.., |..|)`
      (closure-arg variant), compile error. Similar for service/action.
      Spans + diagnostics point at the offending registration.
      **Files**: `packages/core/nros-macros/src/lib.rs` (lint emission).

- **Tests:**
  - [ ] `dispatch_strategy_default_is_inline` — `Node` trait default
        gives `Inline`.
  - [ ] `inline_node_dispatches_via_closure` — existing Inline pattern
        keeps working post-substrate.
  - [ ] `deferred_node_dispatches_via_on_callback` — POSIX-side smoke
        with a synthetic Deferred runtime exercises `signal_callback` +
        `on_callback`.
  - [ ] `lint_rejects_closure_in_deferred_node` — macro emits a clear
        compile error.

### 216.B — RTIC integration

- [ ] **216.B.1** — `RticBoardEntry` trait sibling to `BoardEntry`
      (framework-owned-spin shape):
      ```rust
      pub trait RticBoardEntry: Board {
          type Pac: 'static;
          const DISPATCHERS: &'static [&'static str];

          /// Called from inside the proc-macro-generated `#[init]` body.
          /// Returns the Executor + framework-aware ComponentRuntime
          /// the proc-macro wires into RTIC `#[local]` storage.
          fn init_hardware(
              device: Self::Pac,
              core: cortex_m::Peripherals,
          ) -> (Executor, Self::Runtime);

          type Runtime: ComponentRuntime;
      }
      ```
      Distinct from `BoardEntry` (which keeps the board-owns-spin
      contract for POSIX + RTOS boards).
      **Files**: `packages/core/nros-platform/src/board.rs`.

- [ ] **216.B.2** — `nros-board-rtic-stm32f4` crate:
      `packages/boards/nros-board-rtic-stm32f4/`. Provides:
      * `Pac = stm32f4xx_hal::pac` (chip-specific PAC),
      * `RticStm32F4: Board + BoardInit + RticBoardEntry`,
      * `RticRuntime: ComponentRuntime` w/ `DispatchStrategy::Deferred`
        + `signal_callback` via `heapless::spsc::Producer`,
      * Static SPSC queue declared via `nros_rtic_runtime!` macro from
        `nros-board-rtic-common` (companion crate for shared queue
        machinery + dispatch routing).
      `[package.metadata.nros.board] framework = "rtic"` so
      `nros::main!()` proc-macro discovers the framework kind.
      **Files**: `packages/boards/nros-board-rtic-stm32f4/`,
      `packages/boards/nros-board-rtic-common/` (shared queue +
      dispatch macros).

- [ ] **216.B.3** — `nros::main!()` proc-macro RTIC routing branch:
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
      enumerates registered Components from `run_plan`'s symbol table
      to emit per-Component `#[local]` entries + dispatch routing.
      **Files**: `packages/core/nros-macros/src/main_macro.rs`.

- [ ] **216.B.4** — `nros::main!(custom_tasks = [my_adc, my_ui])`
      syntax. Proc-macro folds extra `#[rtic_task]`-annotated fns into
      the generated `mod __nros_app` body. Token-tree extraction;
      preserve user fn signatures verbatim.
      **Files**: `packages/core/nros-macros/src/main_macro.rs`.

- [ ] **216.B.5** — Migrate `examples/stm32f4/rust/talker-rtic/` to
      `nros::main!()` shape:
      * `src/main.rs` collapses to `nros::main!();`
      * `Cargo.toml` swaps `nros-board-stm32f4` → `nros-board-rtic-stm32f4`,
        adds `[package.metadata.nros.entry] deploy = "rtic-stm32f4"`
      * Companion `talker_pkg/src/lib.rs` Component pkg (board-agnostic)
        with `impl Node for Talker` + `nros::node!(Talker)`. Pub-only;
        `DISPATCH = Inline` works (no callbacks needed).
      **Files**: `examples/stm32f4/rust/talker-rtic/{src/main.rs,
      Cargo.toml}` + new `examples/stm32f4/rust/talker_pkg/`.

- [ ] **216.B.6** — Add `examples/stm32f4/rust/listener-rtic/` —
      callback-driven Component using `DispatchStrategy::Deferred` +
      tag-based subscription. Exercises 216.A.4 + 216.B.3 end-to-end.
      `defmt::info!` from inside `on_callback` proves the Deferred
      dispatch path fires from the `__nros_dispatch` task context (not
      the spin task).
      **Files**: `examples/stm32f4/rust/listener-rtic/`,
      `examples/stm32f4/rust/listener_pkg/`.

- **Tests:**
  - [ ] `phase216_b_rtic_main_macro_expansion` — UI test asserts
        `nros::main!()` for an `rtic-stm32f4` deploy target expands to
        the expected `#[rtic::app]` skeleton.
  - [ ] `phase216_b_rtic_callback_dispatch_e2e` — talker (pub) +
        listener (sub, Deferred) over zenoh-pico loopback on QEMU
        thumbv7m. Listener's `on_callback` fires from
        `__nros_dispatch` task; spin task doesn't reach the callback
        body.

### 216.C — Embassy integration

- [ ] **216.C.1** — `EmbassyBoardEntry` trait sibling to `RticBoardEntry`:
      ```rust
      pub trait EmbassyBoardEntry: Board {
          type Spawner: 'static;
          const CHANNEL_CAPACITY: usize = 32;

          fn init_hardware(spawner: Spawner) -> (Executor, Self::Runtime);
          type Runtime: ComponentRuntime;
      }
      ```
      **Files**: `packages/core/nros-platform/src/board.rs`.

- [ ] **216.C.2** — `nros-board-embassy-stm32f4` crate:
      `packages/boards/nros-board-embassy-stm32f4/`. `EmbassyRuntime`
      uses `embassy_sync::channel::Channel<NoopRawMutex, (CallbackId,
      CallbackCtx), CHANNEL_CAPACITY>` instead of SPSC queue. Channel
      is static; `try_send` from `signal_callback` (non-blocking; drops
      on full + emits log warning).
      `[package.metadata.nros.board] framework = "embassy"`.
      **Files**: `packages/boards/nros-board-embassy-stm32f4/`,
      `packages/boards/nros-board-embassy-common/`.

- [ ] **216.C.3** — `nros::main!()` proc-macro Embassy routing branch:
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

- [ ] **216.C.4** — Migrate `examples/stm32f4/rust/talker-embassy/` to
      `nros::main!()` shape (sibling to 216.B.5).
      **Files**: `examples/stm32f4/rust/talker-embassy/`.

- [ ] **216.C.5** — Add `examples/stm32f4/rust/listener-embassy/` —
      callback-driven Deferred Component (sibling to 216.B.6). Also
      demonstrates the spawn-from-sync escape:
      ```rust
      fn on_callback(&mut self, _cb_id: CallbackId, ctx: CallbackCtx) {
          let msg: Int32 = ctx.downcast().unwrap();
          self.spawner.spawn(handle_downstream(msg)).unwrap();
      }
      ```
      **Files**: `examples/stm32f4/rust/listener-embassy/`.

- **Tests:**
  - [ ] `phase216_c_embassy_main_macro_expansion` — UI test.
  - [ ] `phase216_c_embassy_callback_dispatch_e2e` — sibling to
        216.B's e2e.
  - [ ] `phase216_c_embassy_spawn_from_callback` — verifies the
        spawn-from-sync escape pattern compiles + runs (does NOT verify
        the spawned task completes; that's user code).

### 216.D — `nros check` lint + docs

- [ ] **216.D.1** — `nros check` cross-validates Component
      `DISPATCH` against Entry pkg board framework. Logic:
      ```
      for each Component pkg in workspace:
          strategy = read __nros_node_<pkg>_dispatch_strategy() ABI
          for each Entry pkg deploying <Component>:
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

- [ ] **216.D.2** — Book chapters:
      * `book/src/user-guide/rtic-integration.md` — RTIC tutorial,
        walking from `nros::main!()` to custom-task escape, with
        the Component pkg + Entry pkg shape.
      * `book/src/user-guide/embassy-integration.md` — mirror for
        Embassy, covering the spawn-from-sync escape.
      * `book/src/internals/dispatch-strategy.md` — design rationale
        for the Inline / Deferred / FromIsr trichotomy + the tag-based
        callback API.
      **Files**: `book/src/{user-guide,internals}/*.md` (3 new pages) +
      `book/src/SUMMARY.md`.

### 216.E — Future work (deferred, design slots only)

- [ ] **216.E.1** — `DispatchStrategy::FromIsr` impl. Requires:
      * Reentrancy audit of the spin-loop dispatch path
      * Lock-free SPSC variant tolerant of ISR-priority producer
      * Per-Component `#[isr_safe]` proof contract
      Land when a real ISR-driven driver demands it (e.g. timer pulse
      → `nros::node!()` Component dispatched directly from the timer
      ISR with no scheduler hop).

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

- [ ] **Substrate**: `nros::DispatchStrategy` enum exists + `Node` trait
      carries `DISPATCH` const with `Inline` default + every existing
      Component pkg compiles + tests stay green (substrate is
      additive, not breaking).
- [ ] **RTIC**: `examples/stm32f4/rust/talker-rtic/src/main.rs` collapses
      to `nros::main!();`. Builds + runs end-to-end on real or QEMU
      hardware. `examples/stm32f4/rust/listener-rtic/` exercises
      Deferred dispatch with a real subscription; callback fires from
      the `__nros_dispatch` task (not spin task).
- [ ] **Embassy**: same for `talker-embassy` + new `listener-embassy`.
- [ ] **Lint**: `nros check` rejects a Component pkg with
      `DISPATCH = Inline` deployed to a `framework = "rtic"` board with
      a clear error + suggested fix.
- [ ] **Books**: three new chapters (RTIC, Embassy, dispatch strategy).
- [ ] **Test infra**: `phase216_a_dispatch_strategy.rs` +
      `phase216_b_rtic_*.rs` (×2) + `phase216_c_embassy_*.rs` (×3) all
      pass on a CI lane with QEMU + thumbv7m + Embassy/RTIC toolchains.
- [ ] **Backward compat**: every Phase 212 Component pkg keeps working
      without code changes (Inline default + closure API stay
      operational).

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
  until a real ISR-driven Component lands.
* Cross-references:
  * `docs/design/multi-node-workspace-layout.md` §11.8 (escape hatch)
  * `docs/roadmap/phase-212-ux-cargo-native-and-file-consolidation.md`
    §N (Board trait family + N.9 proc-macro)
  * `docs/roadmap/phase-213-post-212-known-issues.md` §B (cmake fn
    rename — analogous to Phase 216's user-facing-API stability work)
  * `book/src/internals/rmw-backends.md`
  * `book/src/getting-started/integration-stm32f4.md` (existing RTIC
    + Embassy starter; will cross-link Phase 216 chapters)
