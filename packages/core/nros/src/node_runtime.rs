//! Phase 212.M.5.a.2 — Executor-backed `NodeRuntime` /
//! `DeclaredNodeRuntime` for nano-ros.
//!
//! [`MetadataRecorder`](crate::node_metadata::MetadataRecorder)
//! (the planner sink) binds the
//! [`Node`](crate::node::Node) /
//! [`ExecutableNode`](crate::node::ExecutableNode)
//! traits to a pure metadata target. This module is the missing twin:
//! it binds the same traits to a live [`Executor`](crate::Executor) so
//! a Node pkg can actually run — nodes, publishers,
//! subscriptions, timers materialise as real executor handles, and
//! every fired callback dispatches into
//! [`ExecutableNode::on_callback`] with the right
//! [`CallbackId`].
//!
//! Shape:
//!
//! ```ignore
//! use nros::{Executor, ExecutorConfig};
//! use nros::node_runtime::ExecutorNodeRuntime;
//!
//! let cfg = ExecutorConfig::from_env().node_name("talker_main");
//! let executor = Executor::open(&cfg).unwrap();
//! let mut runtime = ExecutorNodeRuntime::from_executor(executor);
//! let _handle = runtime.register_node::<Talker>().unwrap();
//! runtime.spin().unwrap();
//! ```
//!
//! BSP / native-synth consumer (the Phase 212.M.5.a.3 baker — board
//! / native main):
//!
//! ```ignore
//! // Per-pkg register fn ptrs emitted by `nros::node!`.
//! extern "Rust" {
//!     fn __nros_component_talker_pkg_register(
//!         ctx: &mut nros::NodeContext<'_>,
//!     ) -> nros::NodeResult<()>;
//!     fn __nros_component_listener_pkg_register(
//!         ctx: &mut nros::NodeContext<'_>,
//!     ) -> nros::NodeResult<()>;
//! }
//!
//! let executor = Executor::open(&cfg).unwrap();
//! let mut runtime = ExecutorNodeRuntime::from_executor(executor);
//! nros::node_runtime::nros_run_components(
//!     &mut runtime,
//!     &[
//!         __nros_component_talker_pkg_register,
//!         __nros_component_listener_pkg_register,
//!     ],
//! ).unwrap();
//! ```
//!
//! The macro emit signature stays unchanged (M.5.a.1 ABI is frozen).
//! The C-side `system_main.c` from the BSP baker calls a single
//! `extern "Rust"` shim that wraps this — C never sees
//! [`NodeContext`].
//!
//! ## Coverage today (Phase 212.M.5.a.2)
//!
//! Publishers, subscriptions, and repeating timers wire end-to-end:
//! the live executor delivers callbacks; the bound
//! [`ExecutableNode::on_callback`] body runs with a
//! [`CallbackCtx`] backed by the per-component publisher resolver.
//! Service servers / clients and action servers / clients wire
//! end-to-end too (Phase 212.M-F.23): `create_entity` registers them on
//! the executor with C-ABI trampolines that route inbound requests /
//! goals into the component's `on_callback`, and the tick-time client /
//! action surface ([`TickCtx`]) is backed by `RuntimeClientDispatch` /
//! `RuntimeActions` over the live executor. Parameters are still a
//! follow-up (registration succeeds; param callbacks don't fire yet).

#![cfg(feature = "rmw-cffi")]

extern crate alloc;

use alloc::{boxed::Box, string::String, vec::Vec};
use core::{cell::RefCell, marker::PhantomData, time::Duration};

use portable_atomic::{AtomicUsize, Ordering};
use portable_atomic_util::Arc;

use crate::{
    EmbeddedRawPublisher, Executor, GoalId, GoalStatus,
    node::{
        ActionExecutor, Callback, CallbackCtx, ClientDispatch, ExecutableNode, NodeContext,
        NodeDeclError, NodeOptions, NodeResult, NodeRuntime, PublisherResolver, TickCtx,
    },
    node_metadata::{
        CallbackEffectKind, CallbackId, EntityId, EntityKind, EntityMetadata, NodeId as MetaNodeId,
    },
};

// Phase 212.N.7 closing sweep — `component_register_symbol` retired
// (no live callers after the BSP baker + macro extern emit were
// removed). The former re-export here is gone.

// =============================================================================
// Public types
// =============================================================================

/// Opaque handle returned by
/// [`ExecutorNodeRuntime::register_node`].
///
/// `C` distinguishes handles at the type level so a caller who keeps
/// the handle can later (post-M.5.a.3) recover a typed mut-state
/// borrow. For today the handle is purely a witness that registration
/// succeeded.
pub struct RegisteredNode<C: ExecutableNode> {
    component_idx: usize,
    _phantom: PhantomData<fn() -> C>,
}

impl<C: ExecutableNode> RegisteredNode<C> {
    /// Slot index of this component inside the runtime.
    pub fn slot(&self) -> usize {
        self.component_idx
    }
}

/// Errors returned by the runtime entry points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutorError {
    /// One of the components' register / lifecycle calls failed.
    Node(NodeDeclError),
    /// The executor's spin loop returned an unexpected error.
    SpinFailed,
}

impl From<NodeDeclError> for ExecutorError {
    fn from(e: NodeDeclError) -> Self {
        Self::Node(e)
    }
}

// =============================================================================
// Internal slot — type-erases the component's `State` so the runtime
// can hold a heterogeneous vec.
// =============================================================================

trait ComponentSlot {
    fn dispatch(&mut self, cb_id: &str, ctx: &mut CallbackCtx<'_>);
    fn tick(&mut self, ctx: &mut TickCtx<'_>);
}

struct TypedSlot<C: ExecutableNode> {
    state: C::State,
    _phantom: PhantomData<fn() -> C>,
}

impl<C: ExecutableNode> ComponentSlot for TypedSlot<C> {
    fn dispatch(&mut self, cb_id: &str, ctx: &mut CallbackCtx<'_>) {
        C::on_callback(
            &mut self.state,
            Callback::__from_id(CallbackId::new(cb_id)),
            ctx,
        );
    }
    fn tick(&mut self, ctx: &mut TickCtx<'_>) {
        C::tick(&mut self.state, ctx);
    }
}

/// Phase 212.M.5.a.4 — BSP-side dispatch slot.
///
/// The Phase 212.M.5.a.1 macro emit (`__nros_component_<pkg>_register`)
/// drops the concrete component type at the FFI boundary, so the BSP
/// can't reach `ExecutableNode::on_callback` / `::tick` through
/// the register fn alone. M.5.a.4 adds parallel emits — `_init`,
/// `_dispatch`, `_tick` — that the macro generates per component;
/// the BSP baker collects them into parallel fn-pointer tables which
/// pair index-wise with `NROS_REGISTER_FNS`.
///
/// `BspDispatchSlot` holds the type-erased `*mut ()` returned by
/// `_init` (a leaked `Box`) plus the matching dispatch / tick fn
/// pointers; the embedded slot lives for the firmware lifetime so we
/// never `Drop` the boxed state.
pub(crate) struct BspDispatchSlot {
    state: *mut (),
    dispatch: NodeDispatchFn,
    tick: NodeTickFn,
}

// SAFETY: `state` is a `Box`-leaked pointer to the component's `State`.
// The runtime is single-threaded; the slot itself is never shared
// across threads — we hold the `*mut ()` only to forward it to the
// dispatch fn under `&mut self`. Implementing `Send` lets the slot
// sit inside the heterogeneous `Vec<Arc<ComponentCell>>` without
// fighting the auto-trait checker; `Sync` is unnecessary (we never
// share `&BspDispatchSlot` across threads).
unsafe impl Send for BspDispatchSlot {}

impl ComponentSlot for BspDispatchSlot {
    fn dispatch(&mut self, cb_id: &str, ctx: &mut CallbackCtx<'_>) {
        // SAFETY: `self.dispatch` was emitted by `nros::node!()`
        // alongside `self.state` (set at `init` time); the dispatch
        // ABI takes `*mut ()` + internal `CallbackId<'_>` +
        // `&mut CallbackCtx`, and the runtime holds the slot under a
        // `&mut` borrow that serialises calls.
        unsafe {
            (self.dispatch)(self.state, CallbackId::new(cb_id), ctx);
        }
    }
    fn tick(&mut self, ctx: &mut TickCtx<'_>) {
        // SAFETY: same provenance as `dispatch`.
        unsafe {
            (self.tick)(self.state, ctx);
        }
    }
}

/// Shared per-component cell. Subscription / timer closures registered
/// against the executor hold an `Arc` clone so they can dispatch +
/// publish back through the resolver.
struct ComponentCell {
    slot: RefCell<Box<dyn ComponentSlot>>,
    publishers: RefCell<Vec<(String, EmbeddedRawPublisher)>>,
    // Phase 212.M-F.23 — declarative service/action CLIENT + action-SERVER
    // handles, keyed by stable entity id, resolved during tick dispatch.
    // Mirror of the orchestration `GenClientDispatch`/`GenActionExec` arrays,
    // but built at registration time on the single-node runtime. Service- and
    // action-SERVER request/goal dispatch is owned by the executor (the
    // trampolines registered in `create_entity`); only the action-server
    // handle is kept here so the tick can complete goals / publish feedback.
    service_clients: RefCell<Vec<(String, crate::HandleId)>>,
    action_clients: RefCell<Vec<(String, usize)>>,
    action_servers: RefCell<Vec<(String, crate::ActionServerRawHandle)>>,
    callback_dispatches: AtomicUsize,
    message_dispatches: AtomicUsize,
}

impl ComponentCell {
    fn lookup_publisher<R>(
        &self,
        entity_id: &str,
        f: impl FnOnce(&EmbeddedRawPublisher) -> R,
    ) -> Option<R> {
        let pubs = self.publishers.borrow();
        pubs.iter()
            .find(|(id, _)| id == entity_id)
            .map(|(_, p)| f(p))
    }
}

/// `PublisherResolver` implementation backed by a `ComponentCell`.
struct CellResolver<'a> {
    cell: &'a ComponentCell,
}

impl PublisherResolver for CellResolver<'_> {
    fn publish_raw(&self, entity_id: &str, data: &[u8]) -> NodeResult<()> {
        self.cell
            .lookup_publisher(entity_id, |p| {
                p.publish_raw(data).map_err(|_| NodeDeclError::Runtime)
            })
            .unwrap_or(Err(NodeDeclError::Runtime))
    }
}

// Phase 212.M-F.23 — the `UnsupportedActions` / `UnsupportedClients` tick-side
// stubs are retired. Real service/action client + action-server dispatch on the
// single-node runtime lives in `RuntimeClientDispatch` / `RuntimeActions`
// (below), wired into `run_ticks`.

// =============================================================================
// ExecutorNodeRuntime
// =============================================================================

/// Executor-backed component runtime.
///
/// Owns the [`Executor`] and one slot per registered component. The
/// register / spin lifecycle:
///
/// 1. [`from_executor`](Self::from_executor) wraps an open
///    [`Executor`].
/// 2. [`register_node`](Self::register_node) builds the
///    component's `State`, runs [`Node::register`] over an
///    internal [`NodeRuntime`] adapter that materialises nodes /
///    pubs / subs / timers on the real executor, and wires each
///    subscription + timer callback to dispatch into
///    [`ExecutableNode::on_callback`] with the right
///    [`CallbackId`].
/// 3. [`spin`](Self::spin) / [`spin_once`](Self::spin_once) drive the
///    executor; between iterations every registered component's
///    [`ExecutableNode::tick`] runs.
pub struct ExecutorNodeRuntime {
    executor: Executor,
    components: Vec<Arc<ComponentCell>>,
}

impl ExecutorNodeRuntime {
    /// Wrap an already-built [`Executor`].
    pub fn from_executor(executor: Executor) -> Self {
        Self {
            executor,
            components: Vec::new(),
        }
    }

    /// Borrow the underlying executor.
    pub fn executor(&self) -> &Executor {
        &self.executor
    }

    /// Mutably borrow the underlying executor — for advanced wiring
    /// (parameter services, custom guard conditions). Don't use during
    /// [`spin`](Self::spin) from another thread; the runtime is
    /// single-threaded.
    pub fn executor_mut(&mut self) -> &mut Executor {
        &mut self.executor
    }

    /// Number of registered components.
    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    /// Register a [`Node`] (which must also be
    /// [`ExecutableNode`]) into this runtime. Builds the
    /// component's `State` (via [`ExecutableNode::init`]) and
    /// walks [`Node::register`] over the live executor — every
    /// declared node / pub / sub / timer materialises as a real
    /// executor handle, and subscription + timer callbacks are wired
    /// to dispatch into [`ExecutableNode::on_callback`].
    pub fn register_node<C: ExecutableNode + 'static>(&mut self) -> NodeResult<RegisteredNode<C>>
    where
        C::State: 'static,
    {
        let cell = Arc::new(ComponentCell {
            slot: RefCell::new(Box::new(TypedSlot::<C> {
                state: C::init(),
                _phantom: PhantomData,
            })),
            publishers: RefCell::new(Vec::new()),
            service_clients: RefCell::new(Vec::new()),
            action_clients: RefCell::new(Vec::new()),
            action_servers: RefCell::new(Vec::new()),
            callback_dispatches: AtomicUsize::new(0),
            message_dispatches: AtomicUsize::new(0),
        });
        let component_idx = self.components.len();
        self.components.push(cell.clone());

        let mut sink = ExecutorSink {
            executor: &mut self.executor,
            cell: cell.clone(),
            nodes: Vec::new(),
        };
        let sink_dyn: &mut dyn NodeRuntime = &mut sink;
        let mut context = NodeContext::new(C::NAME, sink_dyn);
        let result = C::register(&mut context);
        if result.is_err() {
            // Roll back the slot push so `component_count` stays
            // consistent with what users observe.
            self.components.pop();
        }
        result?;

        Ok(RegisteredNode {
            component_idx,
            _phantom: PhantomData,
        })
    }

    /// Phase 212.M.5.a.4 — BSP entry point: register a single component
    /// against this runtime through the four `extern "Rust"` fn-pointers
    /// the macro emits per pkg. Available on `no_std` (alloc-only) so
    /// the FreeRTOS / NuttX / ThreadX / Zephyr BSP bakers can call it
    /// from their `nros_system_run` loop without depending on the
    /// std-side halt-flag spin in [`nros_run_components`].
    pub fn register_dispatch_slot(
        &mut self,
        register_fn: NodeRegisterFn,
        init_fn: NodeInitFn,
        dispatch_fn: NodeDispatchFn,
        tick_fn: NodeTickFn,
    ) -> Result<(), ExecutorError> {
        let state = (init_fn)();
        let cell = Arc::new(ComponentCell {
            slot: RefCell::new(Box::new(BspDispatchSlot {
                state,
                dispatch: dispatch_fn,
                tick: tick_fn,
            })),
            publishers: RefCell::new(Vec::new()),
            service_clients: RefCell::new(Vec::new()),
            action_clients: RefCell::new(Vec::new()),
            action_servers: RefCell::new(Vec::new()),
            callback_dispatches: AtomicUsize::new(0),
            message_dispatches: AtomicUsize::new(0),
        });
        self.components.push(cell.clone());
        let mut sink = ExecutorSink {
            executor: &mut self.executor,
            cell,
            nodes: Vec::new(),
        };
        let sink_dyn: &mut dyn NodeRuntime = &mut sink;
        let mut context = NodeContext::new("<bsp>", sink_dyn);
        let result = (register_fn)(&mut context);
        if result.is_err() {
            self.components.pop();
        }
        result.map_err(ExecutorError::Node)
    }

    /// Drive one executor iteration + a `tick` per registered
    /// component.
    pub fn spin_once(&mut self, timeout: Duration) -> Result<(), ExecutorError> {
        let _result = self.executor.spin_once(timeout);
        self.run_ticks();
        Ok(())
    }

    /// Phase 216.B.3 / C.3 follow-up — route a signaled callback to
    /// every registered component slot.
    ///
    /// The RTIC (`nros-board-rtic-stm32f4`) and Embassy
    /// (`nros-board-embassy-stm32f4`) dispatch tasks dequeue a
    /// [`nros_platform::SignaledCallback`] envelope from their SPSC
    /// queue / Embassy channel and need a routing entry point that
    /// hands the callback off to the right Node's `on_callback`
    /// trampoline. This method is that entry point.
    ///
    /// # Strategy — linear scan
    ///
    /// Each registered slot's `dispatch_fn` is the codegen-emitted
    /// `d()` trampoline from `nros::node!()` (see
    /// `packages/core/nros-macros/src/lib.rs`). That trampoline calls
    /// `<NodeTy as ExecutableNode>::on_callback`, whose body
    /// `match`es on the callback's own tag set
    /// (`Subscription` / `Timer` / `Service` / `Action` ids) and is a
    /// no-op for non-matching `cb_id`s. So a linear scan across every
    /// slot is correct — each slot self-filters and at most one
    /// component actually acts on a given `cb_id`. A focused
    /// `cb_id → slot` index is a separate follow-up; the trampoline's
    /// tag dispatch already gates the real work cheaply (string
    /// compare on statically known literals), so the linear scan is
    /// the minimum-viable wiring that closes the conceptual gap left
    /// by the B.3 / C.3 skeleton emits.
    ///
    /// # Borrow semantics
    ///
    /// Each `ComponentCell`'s slot lives behind a [`RefCell`]; the
    /// per-slot dispatch takes `try_borrow_mut` and is a no-op on
    /// re-entrancy. The runtime is single-threaded by construction
    /// (the dispatch task owns it via `&mut self`), so the borrow
    /// always succeeds in normal flow.
    pub fn dispatch_callback(&mut self, cb_id: &str, ctx: &mut CallbackCtx<'_>) {
        for cell in &self.components {
            if let Ok(mut slot) = cell.slot.try_borrow_mut() {
                slot.dispatch(cb_id, ctx);
            }
        }
    }

    /// Spin until the executor's halt flag is raised. Hosted-only; on
    /// bare-metal the BSP wraps `spin_once` in its own loop.
    #[cfg(feature = "std")]
    pub fn spin(&mut self) -> Result<(), ExecutorError> {
        // 10 ms tick cadence — matches the existing executor spin
        // budgeting (see `Executor::spin_default`); short enough that
        // component `tick` hooks observe latency under one cycle.
        let tick = Duration::from_millis(10);
        while !self.executor.is_halted() {
            let _ = self.executor.spin_once(tick);
            self.run_ticks();
        }
        Ok(())
    }

    /// Halt a running [`spin`](Self::spin). Idempotent.
    #[cfg(feature = "std")]
    pub fn halt(&self) {
        self.executor.halt();
    }

    fn run_ticks(&mut self) {
        // Per-component tick — each component's resolver is its own cell.
        // Phase 212.M-F.23: the tick reaches the executor (service-client
        // call_raw poll, action-server complete/feedback) through a raw
        // pointer so `&self.components` and `&mut self.executor` (disjoint
        // fields) can be live at once.
        let exec_ptr: *mut Executor = &mut self.executor;
        for cell in &self.components {
            let resolver = CellResolver {
                cell: cell.as_ref(),
            };
            let service_clients = cell.service_clients.borrow();
            let action_clients = cell.action_clients.borrow();
            let action_servers = cell.action_servers.borrow();
            let mut actions = RuntimeActions {
                executor: exec_ptr,
                handles: &action_servers,
            };
            let mut clients = RuntimeClientDispatch {
                executor: exec_ptr,
                services: &service_clients,
                actions: &action_clients,
            };
            let mut ctx = TickCtx::new(&resolver, &mut actions, &mut clients);
            if let Ok(mut slot) = cell.slot.try_borrow_mut() {
                slot.tick(&mut ctx);
            }
        }
    }
}

// =============================================================================
// Phase 212.N.7 step-3.3 — bridge to platform-side `NodeRuntime`.
// =============================================================================
//
// `nros_platform::NodeDispatchRuntime` is the codegen-emitted
// `run_plan(runtime)` body's sink: object-safe + `no_std`. The
// platform layer holds the four per-pkg fn pointers as opaque
// `extern "Rust" fn()` aliases (see
// `packages/core/nros-platform/src/board/runtime.rs`). This impl
// transmutes them back to the typed signatures defined in
// `crate::component_runtime` before forwarding to
// [`ExecutorNodeRuntime::register_dispatch_slot`].
//
// Why the transmute? `nros-platform` must not depend on `nros`
// (that would invert the dep graph). The typed fn-pointer
// signatures live in `nros` because they reference
// `NodeContext`, `CallbackCtx`, `TickCtx`, `CallbackId` —
// nros-only types. The platform-layer aliases anchor at the
// smallest concrete fn type so the macro emit (Phase 212.N.7
// step-3.4) can `mem::transmute` typed fn pointers into the
// opaque alias at the Node-pkg call site.

impl ::nros_platform::NodeDispatchRuntime for ExecutorNodeRuntime {
    fn register_dispatch_slot_dyn(
        &mut self,
        register: ::nros_platform::NodeRegisterFn,
        init: ::nros_platform::NodeInitFn,
        dispatch: ::nros_platform::NodeDispatchFn,
        tick: ::nros_platform::NodeTickFn,
        _name: &'static str,
    ) -> Result<(), ()> {
        // SAFETY: the four opaque `extern "Rust" fn()` aliases at the
        // platform layer were produced by `mem::transmute` from the
        // typed fn-pointer signatures defined in `component_runtime`
        // (the `nros::node!()` macro emit, Phase 212.N.7
        // step-3.4, carries the transmute on the Node-pkg side).
        // We transmute back here at the impl boundary.
        //
        // `extern "Rust" fn()` and `fn(...) -> ...` share the same
        // ABI representation (one pointer); the transmute is purely a
        // type-level reinterpretation of the call site's argument
        // list. The `_name` arg is currently unused — kept on the
        // trait for diagnostics once `ExecutorError::Node`
        // surfaces the pkg name.
        let register_fn: NodeRegisterFn = unsafe { core::mem::transmute(register) };
        let init_fn: NodeInitFn = unsafe { core::mem::transmute(init) };
        let dispatch_fn: NodeDispatchFn = unsafe { core::mem::transmute(dispatch) };
        let tick_fn: NodeTickFn = unsafe { core::mem::transmute(tick) };
        self.register_dispatch_slot(register_fn, init_fn, dispatch_fn, tick_fn)
            .map_err(|_| ())
    }

    fn spin_once(&mut self, timeout_ms: u32) -> Result<(), ()> {
        Self::spin_once(self, Duration::from_millis(timeout_ms.into())).map_err(|_| ())
    }

    fn observed_callback_counts(&self) -> (usize, usize) {
        self.components
            .iter()
            .fold((0, 0), |(callbacks, messages), cell| {
                (
                    callbacks + cell.callback_dispatches.load(Ordering::Relaxed),
                    messages + cell.message_dispatches.load(Ordering::Relaxed),
                )
            })
    }
}

// =============================================================================
// Internal sink — bridges `NodeRuntime` declarations onto the
// live executor.
// =============================================================================

struct ExecutorSink<'a> {
    executor: &'a mut Executor,
    cell: Arc<ComponentCell>,
    /// Per-registration node mapping: stable id → executor `NodeId`.
    nodes: Vec<(String, nros_node::executor::NodeId)>,
}

impl ExecutorSink<'_> {
    fn lookup_node(&self, stable_id: &str) -> Option<nros_node::executor::NodeId> {
        self.nodes
            .iter()
            .find(|(id, _)| id == stable_id)
            .map(|(_, n)| *n)
    }
}

impl NodeRuntime for ExecutorSink<'_> {
    fn create_node(&mut self, id: MetaNodeId<'_>, options: NodeOptions<'_>) -> NodeResult<()> {
        if self.nodes.iter().any(|(s, _)| s.as_str() == id.as_str()) {
            return Err(NodeDeclError::Runtime);
        }
        let node_id = self
            .executor
            .node_builder(options.name)
            .namespace(options.namespace)
            .domain_id(options.domain_id)
            .build()
            .map_err(|_| NodeDeclError::Runtime)?;
        self.nodes.push((String::from(id.as_str()), node_id));
        Ok(())
    }

    fn create_entity(&mut self, metadata: EntityMetadata) -> NodeResult<()> {
        // Phase 228.C tier gate: when this executor runs a specific tier
        // (`active_groups` set by codegen), an entity whose callback group
        // is not active on this tier is a no-op — no RMW handle, no slot.
        // An unlabeled entity (`callback_group == None`) is wildcard-eligible
        // and always registers; the degenerate single-tier executor leaves
        // `active_groups == None`, so every entity registers (byte-identical
        // to pre-228 output).
        if let Some(group) = metadata.callback_group.as_ref()
            && !self.executor.group_active(group.as_str())
        {
            return Ok(());
        }
        let node = self
            .lookup_node(metadata.node_id.as_str())
            .ok_or(NodeDeclError::Runtime)?;
        match metadata.kind {
            EntityKind::Publisher => {
                let handle = self
                    .executor
                    .node_mut(node)
                    .create_generic_publisher(
                        metadata.source_name.as_str(),
                        metadata.type_name,
                        metadata.type_hash,
                    )
                    .map_err(|_| NodeDeclError::Runtime)?;
                let id_owned = String::from(metadata.id.as_str());
                self.cell.publishers.borrow_mut().push((id_owned, handle));
                Ok(())
            }
            EntityKind::Subscription => {
                let cb_id = metadata
                    .callback_id
                    .as_ref()
                    .ok_or(NodeDeclError::Runtime)?;
                let cb_id_owned = String::from(cb_id.as_str());
                let cell = self.cell.clone();
                self.executor
                    .node_mut(node)
                    .create_generic_subscription(
                        metadata.source_name.as_str(),
                        metadata.type_name,
                        metadata.type_hash,
                        move |payload: &[u8]| {
                            dispatch_into_cell(&cell, &cb_id_owned, payload);
                        },
                    )
                    .map_err(|_| NodeDeclError::Runtime)?;
                Ok(())
            }
            EntityKind::Timer => {
                let cb_id = metadata
                    .callback_id
                    .as_ref()
                    .ok_or(NodeDeclError::Runtime)?;
                let cb_id_owned = String::from(cb_id.as_str());
                let period_ms = metadata.period_ms.ok_or(NodeDeclError::Runtime)?;
                let cell = self.cell.clone();
                self.executor
                    .register_timer(
                        nros_node::TimerDuration::from_millis(period_ms),
                        move || {
                            dispatch_into_cell(&cell, &cb_id_owned, &[]);
                        },
                    )
                    .map_err(|_| NodeDeclError::Runtime)?;
                Ok(())
            }
            // Phase 212.M-F.23 — service / action client + server dispatch on
            // the single-node runtime. The executor-level `register_*_on`
            // calls add an arena dispatch entry, so inbound requests / goals
            // are serviced inside `spin_once`; the leaked `*Ctx` trampoline
            // contexts bridge back into the component's `on_callback`. Client
            // handles are stashed in the cell for the tick-side dispatch
            // (`RuntimeClientDispatch` / `RuntimeActions` in `run_ticks`).
            EntityKind::ServiceServer => {
                let cb_id = metadata
                    .callback_id
                    .as_ref()
                    .ok_or(NodeDeclError::Runtime)?;
                let ctx = Box::into_raw(Box::new(ServiceServerCtx {
                    cell: self.cell.clone(),
                    callback_id: String::from(cb_id.as_str()),
                })) as *mut core::ffi::c_void;
                self.executor
                    .register_service_raw_sized_on::<1024, 1024>(
                        node,
                        metadata.source_name.as_str(),
                        metadata.type_name,
                        metadata.type_hash,
                        crate::QosSettings::services_default(),
                        service_server_trampoline,
                        ctx,
                    )
                    .map_err(|_| NodeDeclError::Runtime)?;
                Ok(())
            }
            EntityKind::ServiceClient => {
                let hid = self
                    .executor
                    .register_service_client_raw_sized_on::<1024>(
                        node,
                        metadata.source_name.as_str(),
                        metadata.type_name,
                        metadata.type_hash,
                        crate::QosSettings::services_default(),
                        None,
                        core::ptr::null_mut(),
                    )
                    .map_err(|_| NodeDeclError::Runtime)?;
                self.cell
                    .service_clients
                    .borrow_mut()
                    .push((String::from(metadata.id.as_str()), hid));
                Ok(())
            }
            EntityKind::ActionServer => {
                let goal_cb = metadata
                    .callback_id
                    .as_ref()
                    .ok_or(NodeDeclError::Runtime)?;
                let cancel_cb = metadata
                    .action_cancel_callback_id
                    .as_ref()
                    .ok_or(NodeDeclError::Runtime)?;
                let accepted_cb = metadata
                    .action_accepted_callback_id
                    .as_ref()
                    .map(|c| String::from(c.as_str()));
                let ctx = Box::into_raw(Box::new(ActionServerCtx {
                    cell: self.cell.clone(),
                    goal_callback_id: String::from(goal_cb.as_str()),
                    cancel_callback_id: String::from(cancel_cb.as_str()),
                    accepted_callback_id: accepted_cb,
                })) as *mut core::ffi::c_void;
                let handle = self
                    .executor
                    .register_action_server_raw_sized::<1024, 1024, 1024, 4>(
                        crate::RawActionServerSpec {
                            node_id: Some(node),
                            action_name: metadata.source_name.as_str(),
                            type_name: metadata.type_name,
                            type_hash: metadata.type_hash,
                            qos: crate::QosSettings::services_default(),
                            goal_callback: action_goal_trampoline,
                            cancel_callback: action_cancel_trampoline,
                            accepted_callback: Some(action_accepted_trampoline),
                            context: ctx,
                        },
                    )
                    .map_err(|_| NodeDeclError::Runtime)?;
                self.cell
                    .action_servers
                    .borrow_mut()
                    .push((String::from(metadata.id.as_str()), handle));
                Ok(())
            }
            EntityKind::ActionClient => {
                // A bound `callback_id` (set by
                // `create_action_client_with_callbacks_for_name`) delivers the
                // terminal goal result to the component via `on_callback`; the
                // optional `action_accepted_callback_id` slot carries the
                // feedback callback (reused — unused on a client). The executor
                // auto-drives accept → feedback → result during spin and invokes
                // these trampolines. No callbacks → send-goal only.
                let (result_callback, feedback_callback, ctx) = match metadata.callback_id.as_ref()
                {
                    Some(result_cb) => {
                        let feedback_cb = metadata
                            .action_accepted_callback_id
                            .as_ref()
                            .map(|c| String::from(c.as_str()));
                        let ctx = Box::into_raw(Box::new(ActionClientCtx {
                            cell: self.cell.clone(),
                            result_callback_id: String::from(result_cb.as_str()),
                            feedback_callback_id: feedback_cb.clone(),
                        })) as *mut core::ffi::c_void;
                        let fb = feedback_cb.map(|_| action_feedback_trampoline as _);
                        (Some(action_result_trampoline as _), fb, ctx)
                    }
                    None => (None, None, core::ptr::null_mut()),
                };
                let handle = self
                    .executor
                    .register_action_client_raw_sized::<1024, 1024, 1024>(
                        crate::RawActionClientSpec {
                            node_id: Some(node),
                            action_name: metadata.source_name.as_str(),
                            type_name: metadata.type_name,
                            type_hash: metadata.type_hash,
                            goal_response_callback: None,
                            feedback_callback,
                            result_callback,
                            context: ctx,
                        },
                    )
                    .map_err(|_| NodeDeclError::Runtime)?;
                self.cell
                    .action_clients
                    .borrow_mut()
                    .push((String::from(metadata.id.as_str()), handle.entry_index()));
                Ok(())
            }
            EntityKind::Parameter => {
                // Phase 212.M-F.23 Wave 2 — declarative parameter dispatch on
                // the single-node runtime. The first declared parameter lazily
                // stands up the 6 ROS 2 parameter services for this executor's
                // node; `spin_once` drives those service servers thereafter
                // (`#[cfg(param-services)]` block at spin.rs). The declared
                // source default seeds the value. With `param-services` off the
                // arm is a no-op (entity declared, no RMW handle) — byte-
                // identical to the pre-Wave-2 behavior.
                #[cfg(feature = "param-services")]
                {
                    if self.executor.params().is_none() {
                        self.executor
                            .register_parameter_services()
                            .map_err(|_| NodeDeclError::Runtime)?;
                    }
                    let value = param_default_to_value(metadata.parameter_default.as_ref());
                    self.executor
                        .declare_parameter(metadata.source_name.as_str(), value);
                }
                Ok(())
            }
        }
    }

    fn record_callback_effect(
        &mut self,
        _callback_id: CallbackId<'_>,
        _kind: CallbackEffectKind,
        _entity_id: EntityId<'_>,
    ) -> NodeResult<()> {
        // Planner concern only — the live runtime doesn't need the
        // effect graph at spin time.
        Ok(())
    }
}

/// Lower a source-recorded [`ParameterDefault`] into the executor-facing
/// [`nros_params::ParameterValue`] used to seed a declared parameter. Scalar
/// defaults carry their value directly; the array variants record only the
/// declared type (no element data at the source layer) so they seed as
/// `NotSet` — the parameter is still declared, just without a concrete array
/// default. A `Double` default is stored as a string at the metadata layer and
/// parsed here (unparseable → `0.0`).
#[cfg(feature = "param-services")]
fn param_default_to_value(
    default: Option<&crate::node_metadata::ParameterDefault>,
) -> nros_params::ParameterValue {
    use crate::node_metadata::ParameterDefault;
    use nros_params::ParameterValue;
    match default {
        None => ParameterValue::NotSet,
        Some(ParameterDefault::Bool(b)) => ParameterValue::Bool(*b),
        Some(ParameterDefault::Integer(i)) => ParameterValue::Integer(*i),
        Some(ParameterDefault::Double(s)) => {
            ParameterValue::Double(s.as_str().parse::<f64>().unwrap_or(0.0))
        }
        Some(ParameterDefault::String(s)) => {
            ParameterValue::from_string(s.as_str()).unwrap_or(ParameterValue::NotSet)
        }
        Some(
            ParameterDefault::BoolArray
            | ParameterDefault::IntegerArray
            | ParameterDefault::DoubleArray
            | ParameterDefault::StringArray,
        ) => ParameterValue::NotSet,
    }
}

fn dispatch_into_cell(cell: &Arc<ComponentCell>, cb_id: &str, payload: &[u8]) {
    cell.callback_dispatches.fetch_add(1, Ordering::Relaxed);
    if !payload.is_empty() {
        cell.message_dispatches.fetch_add(1, Ordering::Relaxed);
    }
    let resolver = CellResolver {
        cell: cell.as_ref(),
    };
    let mut ctx = CallbackCtx::new(payload, &resolver);
    // If the slot is already borrowed (a re-entrant publish from a
    // tick hook on the same cell, etc.) we drop this dispatch. In
    // practice `try_borrow_mut` succeeds because subscription / timer
    // callbacks run sequentially under the single-threaded executor.
    if let Ok(mut slot) = cell.slot.try_borrow_mut() {
        slot.dispatch(cb_id, &mut ctx);
    }
}

// =============================================================================
// Phase 212.M-F.23 — service / action SERVER trampolines + tick-side client /
// action dispatch.
//
// The executor's raw service/action-server registration takes C-ABI fn
// pointers, so the runtime leaks a `*Ctx` (lives for the runtime's lifetime,
// like the executor) holding the owning `ComponentCell` + the declared
// callback ids. Each trampoline rebuilds a `CallbackCtx` and routes into the
// component's `on_callback`, exactly as the orchestration codegen's
// `svc_tramp_*` / `goal_tramp_*` do for the Entry path.
// =============================================================================

/// Leaked context for a service-server arena callback.
struct ServiceServerCtx {
    cell: Arc<ComponentCell>,
    callback_id: String,
}

/// Leaked context for an action-server arena callback (goal + cancel + the
/// optional accepted hook all share one).
struct ActionServerCtx {
    cell: Arc<ComponentCell>,
    goal_callback_id: String,
    cancel_callback_id: String,
    accepted_callback_id: Option<String>,
}

/// Leaked context for an action-CLIENT result + feedback callbacks.
struct ActionClientCtx {
    cell: Arc<ComponentCell>,
    result_callback_id: String,
    feedback_callback_id: Option<String>,
}

/// Action-client result callback: the executor's spin auto-drives the goal to
/// completion and hands the terminal result CDR here; route it into the
/// component's `on_callback` (read with `CallbackCtx::message`).
unsafe extern "C" fn action_result_trampoline(
    _goal_id: *const GoalId,
    _status: GoalStatus,
    result_data: *const u8,
    result_len: usize,
    ctx: *mut core::ffi::c_void,
) {
    let actx = unsafe { &*(ctx as *const ActionClientCtx) };
    let result_slice = unsafe { core::slice::from_raw_parts(result_data, result_len) };
    dispatch_into_cell(&actx.cell, &actx.result_callback_id, result_slice);
}

/// Action-client feedback callback: route each feedback CDR into the
/// component's `on_callback` under the bound feedback callback id.
unsafe extern "C" fn action_feedback_trampoline(
    _goal_id: *const GoalId,
    feedback_data: *const u8,
    feedback_len: usize,
    ctx: *mut core::ffi::c_void,
) {
    let actx = unsafe { &*(ctx as *const ActionClientCtx) };
    let Some(cb_id) = actx.feedback_callback_id.as_ref() else {
        return;
    };
    let feedback_slice = unsafe { core::slice::from_raw_parts(feedback_data, feedback_len) };
    dispatch_into_cell(&actx.cell, cb_id, feedback_slice);
}

/// Service-server request callback: deserialize-side runs in the component's
/// `on_callback` via `CallbackCtx::with_reply`; the executor sends the reply
/// from the bytes written into `resp`.
unsafe extern "C" fn service_server_trampoline(
    req: *const u8,
    req_len: usize,
    resp: *mut u8,
    resp_cap: usize,
    resp_len: *mut usize,
    ctx: *mut core::ffi::c_void,
) -> bool {
    let sctx = unsafe { &*(ctx as *const ServiceServerCtx) };
    let req_slice = unsafe { core::slice::from_raw_parts(req, req_len) };
    let resp_slice = unsafe { core::slice::from_raw_parts_mut(resp, resp_cap) };
    let mut written = 0usize;
    let resolver = CellResolver {
        cell: sctx.cell.as_ref(),
    };
    let mut cb = CallbackCtx::with_reply(req_slice, &resolver, resp_slice, &mut written);
    if let Ok(mut slot) = sctx.cell.slot.try_borrow_mut() {
        slot.dispatch(&sctx.callback_id, &mut cb);
    }
    unsafe { *resp_len = written };
    true
}

/// Action-server goal callback → component `on_callback` with a goal decision.
unsafe extern "C" fn action_goal_trampoline(
    _goal_id: *const GoalId,
    goal_data: *const u8,
    goal_len: usize,
    ctx: *mut core::ffi::c_void,
) -> crate::GoalResponse {
    let actx = unsafe { &*(ctx as *const ActionServerCtx) };
    let goal_slice = unsafe { core::slice::from_raw_parts(goal_data, goal_len) };
    let mut resp = crate::GoalResponse::Reject;
    let resolver = CellResolver {
        cell: actx.cell.as_ref(),
    };
    let mut cb = CallbackCtx::with_goal_decision(goal_slice, &resolver, &mut resp);
    if let Ok(mut slot) = actx.cell.slot.try_borrow_mut() {
        slot.dispatch(&actx.goal_callback_id, &mut cb);
    }
    resp
}

/// Action-server cancel callback → component `on_callback` with a cancel
/// decision. The cancel callback has no goal payload.
unsafe extern "C" fn action_cancel_trampoline(
    _goal_id: *const GoalId,
    _status: GoalStatus,
    ctx: *mut core::ffi::c_void,
) -> crate::CancelResponse {
    let actx = unsafe { &*(ctx as *const ActionServerCtx) };
    let mut resp = crate::CancelResponse::Rejected;
    let resolver = CellResolver {
        cell: actx.cell.as_ref(),
    };
    let mut cb = CallbackCtx::with_cancel_decision(&[], &resolver, &mut resp);
    if let Ok(mut slot) = actx.cell.slot.try_borrow_mut() {
        slot.dispatch(&actx.cancel_callback_id, &mut cb);
    }
    resp
}

/// Action-server accepted hook → component `on_callback` (no decision, no
/// payload). No-op when the component didn't declare an accepted callback.
unsafe extern "C" fn action_accepted_trampoline(
    _goal_id: *const GoalId,
    ctx: *mut core::ffi::c_void,
) {
    let actx = unsafe { &*(ctx as *const ActionServerCtx) };
    let Some(cb_id) = actx.accepted_callback_id.as_ref() else {
        return;
    };
    dispatch_into_cell(&actx.cell, cb_id, &[]);
}

/// Tick-side service/action CLIENT dispatch — the single-node runtime's mirror
/// of the orchestration `GenClientDispatch`. Resolves the per-component client
/// handle arrays + a `*mut Executor` (the tick borrows `&components` while
/// needing `&mut executor`, so the executor is reached through a raw pointer,
/// reborrowed `&mut` per call; no aliasing — `executor` and `components` are
/// disjoint fields).
struct RuntimeClientDispatch<'a> {
    executor: *mut Executor,
    services: &'a [(String, crate::HandleId)],
    actions: &'a [(String, usize)],
}

impl RuntimeClientDispatch<'_> {
    fn service(&self, entity: &str) -> NodeResult<crate::HandleId> {
        self.services
            .iter()
            .find(|(e, _)| e == entity)
            .map(|(_, h)| *h)
            .ok_or(NodeDeclError::Runtime)
    }

    fn action_entry(&self, entity: &str) -> NodeResult<usize> {
        self.actions
            .iter()
            .find(|(e, _)| e == entity)
            .map(|(_, i)| *i)
            .ok_or(NodeDeclError::Runtime)
    }
}

impl ClientDispatch for RuntimeClientDispatch<'_> {
    fn call_raw(
        &mut self,
        service_entity: &str,
        request_cdr: &[u8],
        response_buf: &mut [u8],
    ) -> NodeResult<usize> {
        use crate::ServiceClientTrait;
        let hid = self.service(service_entity)?;
        {
            let executor = unsafe { &mut *self.executor };
            let entry = unsafe { executor.service_client_entry_mut(hid.0) }
                .ok_or(NodeDeclError::Runtime)?;
            entry
                .handle
                .send_request_raw(request_cdr)
                .map_err(|_| NodeDeclError::Runtime)?;
        }
        // Bounded wait — caps total time so the tick loop stays responsive.
        for _ in 0..200 {
            let executor = unsafe { &mut *self.executor };
            executor.spin_once(core::time::Duration::from_millis(10));
            let entry = unsafe { executor.service_client_entry_mut(hid.0) }
                .ok_or(NodeDeclError::Runtime)?;
            match entry.handle.try_recv_reply_raw(response_buf) {
                Ok(Some(len)) => return Ok(len),
                Ok(None) => continue,
                Err(_) => return Err(NodeDeclError::Runtime),
            }
        }
        Err(NodeDeclError::Runtime)
    }

    fn send_goal_raw(&mut self, action_entity: &str, goal_cdr: &[u8]) -> NodeResult<GoalId> {
        let entry_index = self.action_entry(action_entity)?;
        let executor = unsafe { &mut *self.executor };
        let core = unsafe { executor.action_client_core_mut(entry_index) }
            .ok_or(NodeDeclError::Runtime)?;
        let goal_id = core
            .send_goal_raw(goal_cdr)
            .map_err(|_| NodeDeclError::Runtime)?;
        // rclcpp-style: request the result immediately. The server queues the
        // get_result request until the goal terminates, then replies — the
        // executor's spin auto-delivers it to the bound result callback (the
        // executor never auto-sends this request, so the declarative client
        // must). Best-effort: a transport hiccup just means no result callback.
        let _ = core.send_get_result_request(&goal_id);
        Ok(goal_id)
    }
}

/// Tick-side action-SERVER execution — mirror of `GenActionExec`. Lets a
/// component complete goals / publish feedback / enumerate active goals from
/// its `tick` via `TickCtx`.
struct RuntimeActions<'a> {
    executor: *mut Executor,
    handles: &'a [(String, crate::ActionServerRawHandle)],
}

impl RuntimeActions<'_> {
    fn handle(&self, entity: &str) -> NodeResult<crate::ActionServerRawHandle> {
        self.handles
            .iter()
            .find(|(e, _)| e == entity)
            .map(|(_, h)| *h)
            .ok_or(NodeDeclError::Runtime)
    }
}

impl ActionExecutor for RuntimeActions<'_> {
    fn complete_goal_raw(
        &mut self,
        action_entity: &str,
        goal_id: &GoalId,
        status: GoalStatus,
        result: &[u8],
    ) -> NodeResult<()> {
        let handle = self.handle(action_entity)?;
        let executor = unsafe { &mut *self.executor };
        handle.complete_goal_raw(executor, goal_id, status, result);
        Ok(())
    }

    fn publish_feedback_raw(
        &mut self,
        action_entity: &str,
        goal_id: &GoalId,
        feedback: &[u8],
    ) -> NodeResult<()> {
        let handle = self.handle(action_entity)?;
        let executor = unsafe { &mut *self.executor };
        handle
            .publish_feedback_raw(executor, goal_id, feedback)
            .map_err(|_| NodeDeclError::Runtime)
    }

    fn for_each_active_goal(
        &self,
        action_entity: &str,
        visit: &mut dyn FnMut(&GoalId, GoalStatus),
    ) {
        if let Ok(handle) = self.handle(action_entity) {
            let executor = unsafe { &*self.executor };
            handle.for_each_active_goal(executor, |g| visit(&g.goal_id, g.status));
        }
    }
}

// =============================================================================
// C-ABI bridge for the M.5.a.3 BSP baker.
// =============================================================================

/// Type of the `extern "Rust"` register fn emitted by
/// [`nros::node!`](crate::node!). The Phase 212.M.5.a.1 macro
/// ABI is frozen; the BSP baker hands an array of these to
/// [`nros_run_components`].
pub type NodeRegisterFn = fn(&mut NodeContext<'_>) -> NodeResult<()>;

/// Phase 212.M.5.a.4 — type of the `extern "Rust"` `_init` fn emitted
/// alongside `_register` by [`nros::node!`](crate::node!).
/// Returns a leaked `Box` pointer to the component's `State`; the
/// BSP slot holds the pointer for the firmware lifetime.
pub type NodeInitFn = fn() -> *mut ();

/// Phase 212.M.5.a.4 — type of the `extern "Rust"` `_dispatch` fn the
/// macro emits per component. Carries the internal callback ID to the
/// generated wrapper, which converts it to product-facing
/// [`Callback`](crate::Callback) before calling `ExecutableNode::on_callback`.
///
/// `unsafe`: the `*mut ()` MUST be a value previously returned by the
/// matching [`NodeInitFn`] and not freed; the BSP holds both in a
/// paired index lookup.
pub type NodeDispatchFn =
    unsafe fn(state: *mut (), callback: CallbackId<'_>, ctx: &mut CallbackCtx<'_>);

/// Phase 212.M.5.a.4 — type of the `extern "Rust"` `_tick` fn the macro
/// emits per component. Wraps `ExecutableNode::tick`. Same
/// `*mut ()` provenance contract as [`NodeDispatchFn`].
pub type NodeTickFn = unsafe fn(state: *mut (), ctx: &mut TickCtx<'_>);

/// BSP shim — register every component against `runtime`, then spin
/// until halt. The Phase 212.M.5.a.3 baker's `system_main.rs` collects
/// the per-pkg `_register` / `_init` / `_dispatch` / `_tick` fn
/// pointers (M.5.a.4) and calls this with four parallel slices: index
/// `i` of each refers to the same component.
///
/// On entry the BSP runs each `_init` to obtain a leaked `Box<State>`
/// pointer, stores it inside a [`BspDispatchSlot`] paired with the
/// matching dispatch / tick fns, and runs the corresponding `_register`
/// under a private [`NodeContext`]. That wires nodes / pubs /
/// subs / timers onto the real executor AND lets the BSP-launched
/// component's `on_callback` / `tick` bodies fire from the spin loop.
#[cfg(feature = "std")]
pub fn nros_run_components(
    runtime: &mut ExecutorNodeRuntime,
    register_fns: &[NodeRegisterFn],
    init_fns: &[NodeInitFn],
    dispatch_fns: &[NodeDispatchFn],
    tick_fns: &[NodeTickFn],
) -> Result<(), ExecutorError> {
    assert_eq!(
        register_fns.len(),
        init_fns.len(),
        "register/init slice mismatch"
    );
    assert_eq!(
        register_fns.len(),
        dispatch_fns.len(),
        "register/dispatch slice mismatch"
    );
    assert_eq!(
        register_fns.len(),
        tick_fns.len(),
        "register/tick slice mismatch"
    );
    for i in 0..register_fns.len() {
        runtime.register_dispatch_slot(
            register_fns[i],
            init_fns[i],
            dispatch_fns[i],
            tick_fns[i],
        )?;
    }
    runtime.spin()
}

// =============================================================================
// Tests
// =============================================================================
//
// Concrete `Executor` construction needs a real RMW backend session
// (with `rmw-cffi` on, `Executor::from_session` takes the cffi
// session). MockSession only exists when `rmw-cffi` is off — so the
// unit tests that exercise live timer firing live in
// `packages/testing/nros-tests/tests/phase212_m5a2_component_runtime.rs`
// gated behind the `component-runtime-test` feature (pulls
// `nros-rmw-zenoh`). The compile-only smoke here verifies the public
// types are reachable through the umbrella surface.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Node;

    #[test]
    fn handle_slot_is_observable() {
        // Trivial smoke — the handle type carries the slot index.
        let h = RegisteredNode::<DummyComp> {
            component_idx: 7,
            _phantom: PhantomData,
        };
        assert_eq!(h.slot(), 7);
    }

    struct DummyComp;
    impl Node for DummyComp {
        const NAME: &'static str = "dummy";
        fn register(_ctx: &mut NodeContext<'_>) -> NodeResult<()> {
            Ok(())
        }
    }
    impl ExecutableNode for DummyComp {
        type State = ();
        fn init() -> Self::State {}
        fn on_callback(_s: &mut (), _cb: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {}
    }
}
