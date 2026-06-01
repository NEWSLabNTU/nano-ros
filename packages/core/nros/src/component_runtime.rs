//! Phase 212.M.5.a.2 — Executor-backed `ComponentRuntime` /
//! `ComponentNodeRuntime` for nano-ros.
//!
//! [`MetadataRecorder`](crate::component_metadata::MetadataRecorder)
//! (the planner sink) binds the
//! [`Component`](crate::component::Component) /
//! [`ExecutableComponent`](crate::component::ExecutableComponent)
//! traits to a pure metadata target. This module is the missing twin:
//! it binds the same traits to a live [`Executor`](crate::Executor) so
//! a Component pkg can actually run — nodes, publishers,
//! subscriptions, timers materialise as real executor handles, and
//! every fired callback dispatches into
//! [`ExecutableComponent::on_callback`] with the right
//! [`CallbackId`].
//!
//! Shape:
//!
//! ```ignore
//! use nros::{Executor, ExecutorConfig};
//! use nros::component_runtime::ExecutorComponentRuntime;
//!
//! let cfg = ExecutorConfig::from_env().node_name("talker_main");
//! let executor = Executor::open(&cfg).unwrap();
//! let mut runtime = ExecutorComponentRuntime::from_executor(executor);
//! let _handle = runtime.register_component::<Talker>().unwrap();
//! runtime.spin().unwrap();
//! ```
//!
//! BSP / native-synth consumer (the Phase 212.M.5.a.3 baker — board
//! / native main):
//!
//! ```ignore
//! // Per-pkg register fn ptrs emitted by `nros::component!`.
//! extern "Rust" {
//!     fn __nros_component_talker_pkg_register(
//!         ctx: &mut nros::ComponentContext<'_>,
//!     ) -> nros::ComponentResult<()>;
//!     fn __nros_component_listener_pkg_register(
//!         ctx: &mut nros::ComponentContext<'_>,
//!     ) -> nros::ComponentResult<()>;
//! }
//!
//! let executor = Executor::open(&cfg).unwrap();
//! let mut runtime = ExecutorComponentRuntime::from_executor(executor);
//! nros::component_runtime::nros_run_components(
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
//! [`ComponentContext`].
//!
//! ## Coverage today (Phase 212.M.5.a.2)
//!
//! Publishers, subscriptions, and repeating timers wire end-to-end:
//! the live executor delivers callbacks; the bound
//! [`ExecutableComponent::on_callback`] body runs with a
//! [`CallbackCtx`] backed by the per-component publisher resolver.
//! Service servers / clients, action servers / clients, and
//! parameters land in M.5.a.4 — `create_entity` accepts the metadata
//! (so component registration succeeds) but the corresponding
//! callbacks aren't fired. Action goal completion + feedback (the
//! [`TickCtx`] surface) is stubbed via [`UnsupportedActions`] until a
//! follow-up wave wires the tick-time action borrow.

#![cfg(feature = "rmw-cffi")]

extern crate alloc;

use alloc::{boxed::Box, string::String, sync::Arc, vec::Vec};
use core::{cell::RefCell, marker::PhantomData, time::Duration};

use crate::{
    EmbeddedRawPublisher, Executor, GoalId, GoalStatus,
    component::{
        ActionExecutor, CallbackCtx, ComponentContext, ComponentError, ComponentResult,
        ComponentRuntime, ExecutableComponent, NodeOptions, PublisherResolver, TickCtx,
    },
    component_metadata::{
        CallbackEffectKind, CallbackId, EntityId, EntityKind, EntityMetadata, NodeId as MetaNodeId,
    },
};

pub use crate::component::component_register_symbol;

// =============================================================================
// Public types
// =============================================================================

/// Opaque handle returned by
/// [`ExecutorComponentRuntime::register_component`].
///
/// `C` distinguishes handles at the type level so a caller who keeps
/// the handle can later (post-M.5.a.3) recover a typed mut-state
/// borrow. For today the handle is purely a witness that registration
/// succeeded.
pub struct ComponentHandle<C: ExecutableComponent> {
    component_idx: usize,
    _phantom: PhantomData<fn() -> C>,
}

impl<C: ExecutableComponent> ComponentHandle<C> {
    /// Slot index of this component inside the runtime.
    pub fn slot(&self) -> usize {
        self.component_idx
    }
}

/// Errors returned by the runtime entry points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutorError {
    /// One of the components' register / lifecycle calls failed.
    Component(ComponentError),
    /// The executor's spin loop returned an unexpected error.
    SpinFailed,
}

impl From<ComponentError> for ExecutorError {
    fn from(e: ComponentError) -> Self {
        Self::Component(e)
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

struct TypedSlot<C: ExecutableComponent> {
    state: C::State,
    _phantom: PhantomData<fn() -> C>,
}

impl<C: ExecutableComponent> ComponentSlot for TypedSlot<C> {
    fn dispatch(&mut self, cb_id: &str, ctx: &mut CallbackCtx<'_>) {
        C::on_callback(&mut self.state, CallbackId::new(cb_id), ctx);
    }
    fn tick(&mut self, ctx: &mut TickCtx<'_>) {
        C::tick(&mut self.state, ctx);
    }
}

/// Erased slot for the BSP path — declarative-only components. The
/// macro emit doesn't expose the `ExecutableComponent::on_callback`
/// body through the `extern "Rust"` register fn (M.5.a.1 ABI is
/// frozen). The Phase 212.M.5.a.3 baker will extend the macro with a
/// parallel dispatch fn-ptr so the BSP path can route callbacks too;
/// until then BSP-launched components register nodes / pubs / subs /
/// timers but their callback bodies don't fire from
/// [`nros_run_components`]. User code that needs callback dispatch
/// calls [`ExecutorComponentRuntime::register_component`] directly.
#[cfg(feature = "std")]
struct DeclarativeSlot;
#[cfg(feature = "std")]
impl ComponentSlot for DeclarativeSlot {
    fn dispatch(&mut self, _cb_id: &str, _ctx: &mut CallbackCtx<'_>) {}
    fn tick(&mut self, _ctx: &mut TickCtx<'_>) {}
}

/// Shared per-component cell. Subscription / timer closures registered
/// against the executor hold an `Arc` clone so they can dispatch +
/// publish back through the resolver.
struct ComponentCell {
    slot: RefCell<Box<dyn ComponentSlot>>,
    publishers: RefCell<Vec<(String, EmbeddedRawPublisher)>>,
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
    fn publish_raw(&self, entity_id: &str, data: &[u8]) -> ComponentResult<()> {
        self.cell
            .lookup_publisher(entity_id, |p| {
                p.publish_raw(data).map_err(|_| ComponentError::Runtime)
            })
            .unwrap_or(Err(ComponentError::Runtime))
    }
}

/// Tick-side `ActionExecutor` stub. Phase 212.M.5.a.2 lands the
/// pub/sub/timer wiring; action goal completion + feedback pubs need
/// the executor borrowed tick-side — left for the M.5.a.4 follow-up
/// so the M.5.a.3 BSP baker can land without depending on it.
struct UnsupportedActions;

impl ActionExecutor for UnsupportedActions {
    fn complete_goal_raw(
        &mut self,
        _action_entity: &str,
        _goal_id: &GoalId,
        _status: GoalStatus,
        _result: &[u8],
    ) -> ComponentResult<()> {
        Err(ComponentError::Runtime)
    }
    fn publish_feedback_raw(
        &mut self,
        _action_entity: &str,
        _goal_id: &GoalId,
        _feedback: &[u8],
    ) -> ComponentResult<()> {
        Err(ComponentError::Runtime)
    }
    fn for_each_active_goal(
        &self,
        _action_entity: &str,
        _visit: &mut dyn FnMut(&GoalId, GoalStatus),
    ) {
    }
}

// =============================================================================
// ExecutorComponentRuntime
// =============================================================================

/// Executor-backed component runtime.
///
/// Owns the [`Executor`] and one slot per registered component. The
/// register / spin lifecycle:
///
/// 1. [`from_executor`](Self::from_executor) wraps an open
///    [`Executor`].
/// 2. [`register_component`](Self::register_component) builds the
///    component's `State`, runs [`Component::register`] over an
///    internal [`ComponentRuntime`] adapter that materialises nodes /
///    pubs / subs / timers on the real executor, and wires each
///    subscription + timer callback to dispatch into
///    [`ExecutableComponent::on_callback`] with the right
///    [`CallbackId`].
/// 3. [`spin`](Self::spin) / [`spin_once`](Self::spin_once) drive the
///    executor; between iterations every registered component's
///    [`ExecutableComponent::tick`] runs.
pub struct ExecutorComponentRuntime {
    executor: Executor,
    components: Vec<Arc<ComponentCell>>,
}

impl ExecutorComponentRuntime {
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

    /// Register a [`Component`] (which must also be
    /// [`ExecutableComponent`]) into this runtime. Builds the
    /// component's `State` (via [`ExecutableComponent::init`]) and
    /// walks [`Component::register`] over the live executor — every
    /// declared node / pub / sub / timer materialises as a real
    /// executor handle, and subscription + timer callbacks are wired
    /// to dispatch into [`ExecutableComponent::on_callback`].
    pub fn register_component<C: ExecutableComponent + 'static>(
        &mut self,
    ) -> ComponentResult<ComponentHandle<C>>
    where
        C::State: 'static,
    {
        let cell = Arc::new(ComponentCell {
            slot: RefCell::new(Box::new(TypedSlot::<C> {
                state: C::init(),
                _phantom: PhantomData,
            })),
            publishers: RefCell::new(Vec::new()),
        });
        let component_idx = self.components.len();
        self.components.push(cell.clone());

        let mut sink = ExecutorSink {
            executor: &mut self.executor,
            cell: cell.clone(),
            nodes: Vec::new(),
        };
        let sink_dyn: &mut dyn ComponentRuntime = &mut sink;
        let mut context = ComponentContext::new(C::NAME, sink_dyn);
        let result = C::register(&mut context);
        if result.is_err() {
            // Roll back the slot push so `component_count` stays
            // consistent with what users observe.
            self.components.pop();
        }
        result?;

        Ok(ComponentHandle {
            component_idx,
            _phantom: PhantomData,
        })
    }

    /// Drive one executor iteration + a `tick` per registered
    /// component.
    pub fn spin_once(&mut self, timeout: Duration) -> Result<(), ExecutorError> {
        let _result = self.executor.spin_once(timeout);
        self.run_ticks();
        Ok(())
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
        // Actions are stubbed (M.5.a.2 ships pub/sub/timer; action goal
        // completion is a follow-up that needs the executor borrowed
        // tick-side).
        for cell in &self.components {
            let resolver = CellResolver {
                cell: cell.as_ref(),
            };
            let mut actions = UnsupportedActions;
            let mut ctx = TickCtx::new(&resolver, &mut actions);
            if let Ok(mut slot) = cell.slot.try_borrow_mut() {
                slot.tick(&mut ctx);
            }
        }
    }
}

// =============================================================================
// Internal sink — bridges `ComponentRuntime` declarations onto the
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

impl ComponentRuntime for ExecutorSink<'_> {
    fn create_node(&mut self, id: MetaNodeId<'_>, options: NodeOptions<'_>) -> ComponentResult<()> {
        if self.nodes.iter().any(|(s, _)| s.as_str() == id.as_str()) {
            return Err(ComponentError::Runtime);
        }
        let node_id = self
            .executor
            .node_builder(options.name)
            .namespace(options.namespace)
            .domain_id(options.domain_id)
            .build()
            .map_err(|_| ComponentError::Runtime)?;
        self.nodes.push((String::from(id.as_str()), node_id));
        Ok(())
    }

    fn create_entity(&mut self, metadata: EntityMetadata) -> ComponentResult<()> {
        let node = self
            .lookup_node(metadata.node_id.as_str())
            .ok_or(ComponentError::Runtime)?;
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
                    .map_err(|_| ComponentError::Runtime)?;
                let id_owned = String::from(metadata.id.as_str());
                self.cell.publishers.borrow_mut().push((id_owned, handle));
                Ok(())
            }
            EntityKind::Subscription => {
                let cb_id = metadata
                    .callback_id
                    .as_ref()
                    .ok_or(ComponentError::Runtime)?;
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
                    .map_err(|_| ComponentError::Runtime)?;
                Ok(())
            }
            EntityKind::Timer => {
                let cb_id = metadata
                    .callback_id
                    .as_ref()
                    .ok_or(ComponentError::Runtime)?;
                let cb_id_owned = String::from(cb_id.as_str());
                let period_ms = metadata.period_ms.ok_or(ComponentError::Runtime)?;
                let cell = self.cell.clone();
                self.executor
                    .register_timer(
                        nros_node::TimerDuration::from_millis(period_ms),
                        move || {
                            dispatch_into_cell(&cell, &cb_id_owned, &[]);
                        },
                    )
                    .map_err(|_| ComponentError::Runtime)?;
                Ok(())
            }
            EntityKind::ServiceServer
            | EntityKind::ServiceClient
            | EntityKind::ActionServer
            | EntityKind::ActionClient
            | EntityKind::Parameter => {
                // M.5.a.2 ships pub / sub / timer wiring. Service / action /
                // parameter dispatch lands in M.5.a.4 — until then registration
                // succeeds (metadata is valid) and the callbacks simply never
                // fire. See the module-level "Coverage today" note.
                Ok(())
            }
        }
    }

    fn record_callback_effect(
        &mut self,
        _callback_id: CallbackId<'_>,
        _kind: CallbackEffectKind,
        _entity_id: EntityId<'_>,
    ) -> ComponentResult<()> {
        // Planner concern only — the live runtime doesn't need the
        // effect graph at spin time.
        Ok(())
    }
}

fn dispatch_into_cell(cell: &Arc<ComponentCell>, cb_id: &str, payload: &[u8]) {
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
// C-ABI bridge for the M.5.a.3 BSP baker.
// =============================================================================

/// Type of the `extern "Rust"` register fn emitted by
/// [`nros::component!`](crate::component). The Phase 212.M.5.a.1 macro
/// ABI is frozen; the BSP baker hands an array of these to
/// [`nros_run_components`].
pub type ComponentRegisterFn = fn(&mut ComponentContext<'_>) -> ComponentResult<()>;

/// BSP shim — register every component in `register_fns` against
/// `runtime`, then spin until halt. The Phase 212.M.5.a.3 baker's
/// cmake-emitted `system_main.c` collects the per-pkg register fn
/// pointers and calls into a single Rust shim, which holds the C ABI
/// mismatch on the Rust side and never exposes [`ComponentContext`]
/// across the FFI boundary.
///
/// Each register fn is invoked under a fresh [`ComponentContext`]
/// backed by a private one-shot [`ComponentRuntime`] sink — that
/// gives every pkg its own declaration-time view of the runtime
/// without the BSP needing to know the per-pkg `Component` type. The
/// callback bodies are NOT routed from this path today (see
/// [`DeclarativeSlot`] and the M.5.a.3 follow-up); call sites that
/// need full callback dispatch use
/// [`ExecutorComponentRuntime::register_component`] directly with the
/// concrete component type.
#[cfg(feature = "std")]
pub fn nros_run_components(
    runtime: &mut ExecutorComponentRuntime,
    register_fns: &[ComponentRegisterFn],
) -> Result<(), ExecutorError> {
    for f in register_fns {
        let cell = Arc::new(ComponentCell {
            slot: RefCell::new(Box::new(DeclarativeSlot)),
            publishers: RefCell::new(Vec::new()),
        });
        runtime.components.push(cell.clone());
        let mut sink = ExecutorSink {
            executor: &mut runtime.executor,
            cell,
            nodes: Vec::new(),
        };
        let sink_dyn: &mut dyn ComponentRuntime = &mut sink;
        // The component's canonical NAME is set by Component::register;
        // the placeholder here is diagnostic-only.
        let mut context = ComponentContext::new("<bsp>", sink_dyn);
        (f)(&mut context).map_err(ExecutorError::Component)?;
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
    use crate::component::Component;

    #[test]
    fn handle_slot_is_observable() {
        // Trivial smoke — the handle type carries the slot index.
        let h = ComponentHandle::<DummyComp> {
            component_idx: 7,
            _phantom: PhantomData,
        };
        assert_eq!(h.slot(), 7);
    }

    struct DummyComp;
    impl Component for DummyComp {
        const NAME: &'static str = "dummy";
        fn register(_ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
            Ok(())
        }
    }
    impl ExecutableComponent for DummyComp {
        type State = ();
        fn init() -> Self::State {}
        fn on_callback(_s: &mut (), _cb: CallbackId<'_>, _ctx: &mut CallbackCtx<'_>) {}
    }
}
