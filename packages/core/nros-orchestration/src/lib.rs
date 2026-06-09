//! Runtime support for generated nano-ros orchestration packages.
//!
//! This crate contains target-side typed specs only. Host tools and generated
//! package `build.rs` code read `nros-plan.json`; firmware uses the Rust tables
//! emitted from that plan.

#![no_std]

#[cfg(feature = "std")]
extern crate std;

use nros_node::HandleId;

/// Stable identifier embedded in generated firmware for plan/runtime tracing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlanId(pub u32);

/// Static executor and table capacities derived from the checked plan.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CapacitySpec {
    pub max_nodes: usize,
    pub max_callbacks: usize,
    pub max_sched_contexts: usize,
    pub max_parameters: usize,
    pub max_interfaces: usize,
}

/// Node implementation linked into the generated binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentSpec {
    pub id: &'static str,
    pub package: &'static str,
    pub symbol: &'static str,
    pub language: ComponentLanguage,
}

/// Source language for a linked component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentLanguage {
    Rust,
    C,
    Cpp,
}

/// One launch-derived component instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstanceSpec {
    pub id: &'static str,
    pub component_id: &'static str,
    pub node_name: &'static str,
    pub namespace: &'static str,
    pub domain_id: Option<u32>,
    pub parameter_start: usize,
    pub parameter_len: usize,
}

/// One launch-resolved node inside a component instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeSpec {
    pub instance_id: &'static str,
    pub node_id: &'static str,
    pub source_node: &'static str,
    pub node_name: &'static str,
    pub namespace: &'static str,
    pub domain_id: Option<u32>,
}

/// Final parameter value emitted by the planner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParameterSpec {
    pub instance_id: &'static str,
    pub name: &'static str,
    pub value: ParameterValue,
}

/// Static parameter value representation for generated tables.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParameterValue {
    Bool(bool),
    I64(i64),
    F64(f64),
    Str(&'static str),
}

/// Plan-level scheduling context spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedContextSpec {
    pub id: &'static str,
    pub class: SchedClassSpec,
    pub priority: PrioritySpec,
    pub period_us: Option<u32>,
    pub budget_us: Option<u32>,
    pub deadline_us: Option<u32>,
    pub deadline_policy: DeadlinePolicySpec,
    pub os_pri: u8,
    pub tt_window_offset_us: Option<u32>,
    pub tt_window_duration_us: Option<u32>,
}

impl SchedContextSpec {
    pub const fn fifo(id: &'static str) -> Self {
        Self {
            id,
            class: SchedClassSpec::Fifo,
            priority: PrioritySpec::Normal,
            period_us: None,
            budget_us: None,
            deadline_us: None,
            deadline_policy: DeadlinePolicySpec::Activated,
            os_pri: 0,
            tt_window_offset_us: None,
            tt_window_duration_us: None,
        }
    }

    #[cfg(feature = "rmw-cffi")]
    pub fn to_nros_node(self) -> nros_node::executor::sched_context::SchedContext {
        nros_node::executor::sched_context::SchedContext {
            class: self.class.into(),
            priority: self.priority.into(),
            period_us: opt_us(self.period_us),
            budget_us: opt_us(self.budget_us),
            deadline_us: opt_us(self.deadline_us),
            deadline_policy: self.deadline_policy.into(),
            os_pri: self.os_pri,
            tt_window_offset_us: opt_us(self.tt_window_offset_us),
            tt_window_duration_us: opt_us(self.tt_window_duration_us),
        }
    }
}

/// Scheduler class mirror kept stable for generated target tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedClassSpec {
    Fifo,
    Edf,
    Sporadic,
    BestEffort,
}

#[cfg(feature = "rmw-cffi")]
impl From<SchedClassSpec> for nros_node::executor::sched_context::SchedClass {
    fn from(value: SchedClassSpec) -> Self {
        match value {
            SchedClassSpec::Fifo => Self::Fifo,
            SchedClassSpec::Edf => Self::Edf,
            SchedClassSpec::Sporadic => Self::Sporadic,
            SchedClassSpec::BestEffort => Self::BestEffort,
        }
    }
}

/// Priority bucket mirror kept stable for generated target tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrioritySpec {
    Critical,
    Normal,
    BestEffort,
}

#[cfg(feature = "rmw-cffi")]
impl From<PrioritySpec> for nros_node::executor::sched_context::Priority {
    fn from(value: PrioritySpec) -> Self {
        match value {
            PrioritySpec::Critical => Self::Critical,
            PrioritySpec::Normal => Self::Normal,
            PrioritySpec::BestEffort => Self::BestEffort,
        }
    }
}

/// EDF deadline interpretation mirror kept stable for generated target tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlinePolicySpec {
    Released,
    Activated,
    Inherited,
}

#[cfg(feature = "rmw-cffi")]
impl From<DeadlinePolicySpec> for nros_node::executor::sched_context::DeadlinePolicy {
    fn from(value: DeadlinePolicySpec) -> Self {
        match value {
            DeadlinePolicySpec::Released => Self::Released,
            DeadlinePolicySpec::Activated => Self::Activated,
            DeadlinePolicySpec::Inherited => Self::Inherited,
        }
    }
}

/// A plan callback bound to a plan scheduling context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallbackBindingSpec {
    pub callback_index: usize,
    pub sched_context_index: usize,
}

/// Fixed callback handle table filled by generated component constructors.
#[derive(Debug, Clone, Copy)]
pub struct CallbackHandleTable<const N: usize> {
    handles: [Option<HandleId>; N],
}

impl<const N: usize> CallbackHandleTable<N> {
    pub const fn new() -> Self {
        Self { handles: [None; N] }
    }

    pub fn set(&mut self, index: usize, handle: HandleId) -> Result<(), OrchestrationError> {
        let slot = self
            .handles
            .get_mut(index)
            .ok_or(OrchestrationError::CallbackIndexOutOfRange)?;
        *slot = Some(handle);
        Ok(())
    }

    pub fn get(&self, index: usize) -> Option<HandleId> {
        self.handles.get(index).copied().flatten()
    }
}

impl<const N: usize> Default for CallbackHandleTable<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Complete target-side system table emitted by generated package build.rs.
#[derive(Debug, Clone, Copy)]
pub struct SystemSpec {
    pub schema: &'static str,
    pub plan_id: PlanId,
    pub capacities: CapacitySpec,
    pub components: &'static [ComponentSpec],
    pub instances: &'static [InstanceSpec],
    pub nodes: &'static [NodeSpec],
    pub parameters: &'static [ParameterSpec],
    pub sched_contexts: &'static [SchedContextSpec],
    pub callback_bindings: &'static [CallbackBindingSpec],
}

impl SystemSpec {
    pub fn default_node_name(&self) -> &'static str {
        self.instances
            .first()
            .map(|instance| instance.node_name)
            .unwrap_or("nros_system")
    }
}

/// Phase 172.I — a fixed-size shared byte region that co-located components in
/// one generated binary read/write (a blackboard); the generator emits one
/// `static` per `nros.toml` `[[shared_state]]` entry, and a component overlays
/// its own typed view onto the bytes.
///
/// **Access discipline (not a lock).** nano-ros executors dispatch callbacks
/// cooperatively on a single spin thread, so accesses do not overlap — `with`
/// hands out a `&mut [u8; N]` directly, dependency-free. Do **not** hold two
/// `with` borrows at once, and on a future preemptive/multi-tier executor wrap
/// access in the platform critical section.
pub struct SharedRegion<const N: usize> {
    cell: core::cell::UnsafeCell<[u8; N]>,
}

// SAFETY: cooperative single-threaded dispatch (see the access-discipline note);
// `with` is the only accessor and never yields a borrow across a callback.
unsafe impl<const N: usize> Sync for SharedRegion<N> {}

impl<const N: usize> SharedRegion<N> {
    /// Create a zero-initialized region (const, for `static` placement).
    pub const fn new() -> Self {
        Self {
            cell: core::cell::UnsafeCell::new([0u8; N]),
        }
    }

    /// Run `f` with mutable access to the region's bytes.
    pub fn with<R>(&self, f: impl FnOnce(&mut [u8; N]) -> R) -> R {
        // SAFETY: see the type-level access-discipline note — non-overlapping
        // access under cooperative single-threaded dispatch.
        unsafe { f(&mut *self.cell.get()) }
    }

    /// Region size in bytes.
    pub const fn len(&self) -> usize {
        N
    }

    pub const fn is_empty(&self) -> bool {
        N == 0
    }
}

impl<const N: usize> Default for SharedRegion<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Phase 228.D.2 — the **cross-tier** counterpart to [`SharedRegion`]. When a
/// `system.toml [[shared_state]]` region's accessors span more than one
/// scheduling tier (RFC-0015 §8.3, `sync = "mutex"` / `tier_aware` on a
/// multi-tier system), preemptive RTOS tasks can touch it concurrently, so the
/// single-threaded access discipline no longer holds. This variant wraps every
/// access in `critical_section::with` — the platform supplies the impl (RTOS =
/// interrupt mask / kernel guard, bare-metal = PRIMASK, native = std mutex),
/// the same primitive `nros-rmw-zenoh`'s `ffi-sync` already relies on. Codegen
/// selects this type instead of [`SharedRegion`] when the resolved sync policy
/// is `mutex` / `critical_section`; single-tier `none` keeps the lock-free
/// [`SharedRegion`] (byte-identical to today).
pub struct LockedSharedRegion<const N: usize> {
    cell: core::cell::UnsafeCell<[u8; N]>,
}

// SAFETY: every access goes through `critical_section::with`, so no two `with`
// closures run concurrently even under preemptive multi-tier dispatch.
unsafe impl<const N: usize> Sync for LockedSharedRegion<N> {}

impl<const N: usize> LockedSharedRegion<N> {
    /// Create a zero-initialized region (const, for `static` placement).
    pub const fn new() -> Self {
        Self {
            cell: core::cell::UnsafeCell::new([0u8; N]),
        }
    }

    /// Run `f` with mutable access to the region's bytes, under the platform
    /// critical section. The closure must be short — it runs with the guard
    /// held (interrupts masked on bare-metal / single-core RTOS).
    pub fn with<R>(&self, f: impl FnOnce(&mut [u8; N]) -> R) -> R {
        critical_section::with(|_cs| {
            // SAFETY: the critical section serializes all accessors, so this is
            // the only live borrow of the cell for the closure's duration.
            unsafe { f(&mut *self.cell.get()) }
        })
    }

    /// Region size in bytes.
    pub const fn len(&self) -> usize {
        N
    }

    pub const fn is_empty(&self) -> bool {
        N == 0
    }
}

impl<const N: usize> Default for LockedSharedRegion<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Runtime orchestration errors that are independent of transport errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrchestrationError {
    CallbackIndexOutOfRange,
    MissingCallbackHandle,
}

#[cfg(feature = "rmw-cffi")]
fn opt_us(value: Option<u32>) -> nros_node::executor::sched_context::OptUs {
    match value {
        Some(us) => nros_node::executor::sched_context::OptUs::from_us(us),
        None => nros_node::executor::sched_context::OptUs::NONE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "rmw-cffi")]
    #[test]
    fn sched_context_spec_maps_to_executor_type() {
        use nros_node::executor::sched_context::{DeadlinePolicy, Priority, SchedClass};

        let spec = SchedContextSpec {
            id: "control",
            class: SchedClassSpec::Edf,
            priority: PrioritySpec::Critical,
            period_us: Some(10_000),
            budget_us: Some(1_000),
            deadline_us: Some(8_000),
            deadline_policy: DeadlinePolicySpec::Released,
            os_pri: 42,
            tt_window_offset_us: Some(2_000),
            tt_window_duration_us: Some(4_000),
        };

        let sc = spec.to_nros_node();
        assert_eq!(sc.class, SchedClass::Edf);
        assert_eq!(sc.priority, Priority::Critical);
        assert_eq!(sc.period_us.raw(), 10_000);
        assert_eq!(sc.budget_us.raw(), 1_000);
        assert_eq!(sc.deadline_us.raw(), 8_000);
        assert_eq!(sc.deadline_policy, DeadlinePolicy::Released);
        assert_eq!(sc.os_pri, 42);
        assert_eq!(sc.tt_window_offset_us.raw(), 2_000);
        assert_eq!(sc.tt_window_duration_us.raw(), 4_000);
    }

    // Mirrors the exact shape `render_shared_state` emits into generated code.
    static SHARED_BLACKBOARD: SharedRegion<8> = SharedRegion::new();

    #[test]
    fn shared_region_static_zero_init_and_mutate() {
        assert_eq!(SHARED_BLACKBOARD.len(), 8);
        assert!(!SHARED_BLACKBOARD.is_empty());
        SHARED_BLACKBOARD.with(|bytes| assert_eq!(*bytes, [0u8; 8]));
        SHARED_BLACKBOARD.with(|bytes| bytes[0] = 0xAB);
        let read = SHARED_BLACKBOARD.with(|bytes| bytes[0]);
        assert_eq!(read, 0xAB);
    }

    // `LockedSharedRegion::with` needs a platform `critical-section` impl (the
    // RTOS / native runtime provides it at link time), so the unit test covers
    // only the `const` surface; the guarded access is exercised by the
    // cross-tier orchestration integration where the impl is present.
    static LOCKED_BLACKBOARD: LockedSharedRegion<16> = LockedSharedRegion::new();

    #[test]
    fn locked_shared_region_const_surface() {
        assert_eq!(LOCKED_BLACKBOARD.len(), 16);
        assert!(!LOCKED_BLACKBOARD.is_empty());
        assert!(LockedSharedRegion::<0>::new().is_empty());
    }
    // The cross-tier guard's serialization is the `critical_section::with`
    // contract (the platform supplies the impl); a host concurrency test would
    // need a `critical-section/std` dev-dep, which conflicts with the
    // bare-metal impl's restore-state under workspace feature unification — so
    // the behavioral guard proof lives in the orchestration integration, not a
    // unit test here.

    #[test]
    fn callback_handle_table_tracks_registered_handles() {
        let mut table = CallbackHandleTable::<2>::new();
        assert_eq!(table.get(0), None);
        table.set(1, HandleId(7)).unwrap();
        assert_eq!(table.get(1), Some(HandleId(7)));
        assert_eq!(
            table.set(2, HandleId(9)),
            Err(OrchestrationError::CallbackIndexOutOfRange)
        );
    }
}
