//! Executor struct and core spin methods.

use core::{marker::PhantomData, mem::MaybeUninit};

use nros_core::{RosMessage, RosService};
use nros_rmw::{QosSettings, ServiceInfo, Session, TopicInfo, TransportError};

use crate::{session, timer::TimerDuration};

#[cfg(feature = "safety-e2e")]
use super::arena::{
    SubSafetyEntry, sub_safety_has_data, sub_safety_pre_sample, sub_safety_try_process,
};
#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi",
    feature = "rmw-uorb"
))]
use super::types::ExecutorConfig;
#[cfg(feature = "std")]
use super::types::SpinOptions;
use super::{
    arena::{
        BufferStrategy, CallbackMeta, EntryKind, GuardConditionEntry, ServiceClientRawArenaEntry,
        SrvEntry, SrvRawEntry, SubBufferedEntry, SubBufferedRawCEntry, SubBufferedRawEntry,
        SubInfoEntry, TimerEntry, TimerHeader, always_ready, buffered_region_size, drop_entry,
        guard_has_data, guard_try_process, no_pre_sample, service_client_raw_try_process,
        srv_has_data, srv_raw_has_data, srv_raw_try_process, srv_try_process,
        sub_buffered_has_data, sub_buffered_raw_c_has_data, sub_buffered_raw_c_try_process,
        sub_buffered_raw_has_data, sub_buffered_raw_try_process, sub_buffered_try_process,
        sub_info_has_data, sub_info_pre_sample, sub_info_try_process, timer_try_process,
    },
    node::Node,
    spsc_ring::SpscRing,
    triple_buffer::TripleBuffer,
    types::{
        ExecutorSemantics, GuardConditionHandle, HandleId, InvocationMode, NodeError,
        RawResponseCallback, RawServiceCallback, RawSubscriptionCallback, ReadinessSnapshot,
        SpinOnceResult, SpinPeriodPollingResult, Trigger,
    },
};

// ============================================================================
// Executor::open() factory method
// ============================================================================

#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi",
    feature = "rmw-uorb"
))]
impl Executor {
    /// Open a new executor session using the active RMW backend.
    ///
    /// Connects to the middleware at the locator specified in `config`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = ExecutorConfig::from_env().node_name("my_node");
    /// let mut executor = Executor::open(&config)?;
    /// ```
    pub fn open(config: &ExecutorConfig<'_>) -> Result<Self, NodeError> {
        #[cfg(feature = "rmw-zenoh")]
        {
            let tc = nros_rmw::TransportConfig {
                locator: Some(config.locator),
                mode: config.mode,
                properties: &[],
            };
            let session = nros_rmw_zenoh::ZenohSession::new(&tc)
                .map_err(|_| NodeError::Transport(TransportError::ConnectionFailed))?;
            let mut executor = Self::from_session(session);
            executor.set_node_identity(config.node_name, config.namespace);
            Ok(executor)
        }
        #[cfg(feature = "rmw-xrce")]
        {
            use nros_rmw::Rmw;

            // Wait for network on platforms that need it
            #[cfg(feature = "platform-zephyr")]
            {
                unsafe extern "C" {
                    fn xrce_zephyr_wait_network(timeout_ms: core::ffi::c_int) -> i32;
                }
                unsafe { xrce_zephyr_wait_network(5000) };
            }

            // Auto-init transport based on active feature
            #[cfg(feature = "platform-udp")]
            unsafe {
                nros_rmw_xrce::platform_udp::init_platform_udp_transport(config.locator);
            }
            #[cfg(feature = "posix-serial")]
            unsafe {
                nros_rmw_xrce::platform_serial::init_platform_serial_transport(config.locator);
            }

            let rmw_config = nros_rmw::RmwConfig {
                locator: config.locator,
                mode: config.mode,
                domain_id: config.domain_id,
                node_name: config.node_name,
                namespace: config.namespace,
                properties: &[],
            };
            let session = nros_rmw_xrce::XrceRmw::default()
                .open(&rmw_config)
                .map_err(|_| NodeError::Transport(TransportError::ConnectionFailed))?;
            let mut executor = Self::from_session(session);
            executor.set_node_identity(config.node_name, config.namespace);
            Ok(executor)
        }
        #[cfg(feature = "rmw-dds")]
        {
            use nros_rmw::Rmw;

            let rmw_config = nros_rmw::RmwConfig {
                locator: config.locator,
                mode: config.mode,
                domain_id: config.domain_id,
                node_name: config.node_name,
                namespace: config.namespace,
                properties: &[],
            };
            let session = nros_rmw_dds::DdsRmw
                .open(&rmw_config)
                .map_err(|_| NodeError::Transport(TransportError::ConnectionFailed))?;
            let mut executor = Self::from_session(session);
            executor.set_node_identity(config.node_name, config.namespace);
            Ok(executor)
        }
        #[cfg(feature = "rmw-cffi")]
        {
            use nros_rmw::Rmw;

            let rmw_config = nros_rmw::RmwConfig {
                locator: config.locator,
                mode: config.mode,
                domain_id: config.domain_id,
                node_name: config.node_name,
                namespace: config.namespace,
                properties: &[],
            };
            let session = nros_rmw_cffi::CffiRmw
                .open(&rmw_config)
                .map_err(|_| NodeError::Transport(TransportError::ConnectionFailed))?;
            let mut executor = Self::from_session(session);
            executor.set_node_identity(config.node_name, config.namespace);
            Ok(executor)
        }
        #[cfg(feature = "rmw-uorb")]
        {
            use nros_rmw::Rmw;

            let rmw_config = nros_rmw::RmwConfig {
                locator: config.locator,
                mode: config.mode,
                domain_id: config.domain_id,
                node_name: config.node_name,
                namespace: config.namespace,
                properties: &[],
            };
            let session = nros_rmw_uorb::UorbRmw
                .open(&rmw_config)
                .map_err(|_| NodeError::Transport(TransportError::ConnectionFailed))?;
            let mut executor = Self::from_session(session);
            executor.set_node_identity(config.node_name, config.namespace);
            Ok(executor)
        }
    }
}

// ============================================================================
// SessionStore — owned or borrowed session
// ============================================================================

/// Session storage: owned or borrowed via raw pointer.
///
/// The C API creates a session in `nros_support_init()` before the
/// executor. `Borrowed` lets the executor use that session without owning it.
#[allow(clippy::large_enum_variant)]
pub(crate) enum SessionStore {
    Owned(session::ConcreteSession),
    Borrowed(*mut session::ConcreteSession),
}

impl core::ops::Deref for SessionStore {
    type Target = session::ConcreteSession;
    fn deref(&self) -> &session::ConcreteSession {
        match self {
            SessionStore::Owned(s) => s,
            SessionStore::Borrowed(ptr) => unsafe { &**ptr },
        }
    }
}

impl core::ops::DerefMut for SessionStore {
    fn deref_mut(&mut self) -> &mut session::ConcreteSession {
        match self {
            SessionStore::Owned(s) => s,
            SessionStore::Borrowed(ptr) => unsafe { &mut **ptr },
        }
    }
}

// ============================================================================
// Executor
// ============================================================================

/// Backend-agnostic executor that owns a session.
///
/// Provides `create_node()` for entity creation and `drive_io()` for polling.
///
/// # Callback Mode
///
/// The executor supports arena-based callback registration via
/// [`add_subscription()`](Self::add_subscription) and
/// [`add_service()`](Self::add_service), with dispatch via
/// [`spin_once()`](Self::spin_once). No heap allocation is needed.
///
/// The sizes are set via `NROS_EXECUTOR_MAX_CBS` (default 4) and
/// `NROS_EXECUTOR_ARENA_SIZE` (default 4096) environment variables at build time.
pub struct Executor {
    pub(crate) session: SessionStore,
    pub(crate) arena: [MaybeUninit<u8>; crate::config::ARENA_SIZE],
    pub(crate) arena_used: usize,
    pub(crate) entries: [Option<CallbackMeta>; crate::config::MAX_CBS],
    /// Phase 110.B — registered scheduling contexts. Slot 0 is
    /// auto-populated with a `Fifo` SC at construction; every entry
    /// without an explicit binding maps to it via
    /// `sched_context_bindings`.
    pub(crate) sched_contexts:
        [Option<super::sched_context::SchedContext>; crate::config::MAX_SC],
    /// Per-entry SC binding parallel to `entries`. Defaults to
    /// `SchedContextId(0)` (the auto-created Fifo SC).
    pub(crate) sched_context_bindings:
        [super::sched_context::SchedContextId; crate::config::MAX_CBS],
    /// Phase 110.E — user-space sporadic-server budget state per
    /// Sporadic-class SC. Slot indices match `sched_contexts`; non-
    /// Sporadic slots stay `None`.
    pub(crate) sporadic_states:
        [Option<super::sched_context::SporadicState>; crate::config::MAX_SC],
    /// Phase 110.E.b — atomic sporadic state + opaque platform-timer
    /// handle for ISR-driven refill. Populated by
    /// `register_sporadic_timer`; dropped on Executor `Drop` via the
    /// stored `destroy_fn`.
    #[cfg(feature = "alloc")]
    pub(crate) sporadic_atomic_states:
        [Option<(
            alloc::sync::Arc<super::sched_context::AtomicSporadicState>,
            OpaqueTimerHandle,
        )>; crate::config::MAX_SC],
    /// Phase 110.G — major-frame length for time-triggered dispatch.
    /// `0` (default) disables the TT gate entirely; non-zero enables
    /// gating per
    /// `SchedContext.tt_window_offset_us / tt_window_duration_us`.
    pub(crate) major_frame_us: u32,
    pub(crate) trigger: Trigger,
    pub(crate) semantics: ExecutorSemantics,
    /// Node name for entities created via `add_subscription`/`add_service`.
    /// Empty means unset — no liveliness tokens will be declared.
    pub(crate) node_name: heapless::String<64>,
    /// Node namespace (default: "/").
    pub(crate) namespace: heapless::String<64>,
    #[cfg(feature = "std")]
    pub(crate) halt_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    #[cfg(feature = "param-services")]
    pub(crate) params: Option<alloc::boxed::Box<crate::parameter_services::ParamState>>,
    #[cfg(feature = "lifecycle-services")]
    pub(crate) lifecycle:
        Option<alloc::boxed::Box<crate::lifecycle_services::LifecycleRuntimeState>>,
    /// Sub-millisecond wall-clock residual carried across `spin_once` calls
    /// so timers tick at true wall-clock rate even when `drive_io` returns
    /// in well under 1 ms (e.g. zenoh-pico condvar wakeups under load).
    #[cfg(feature = "std")]
    pub(crate) spin_residual_us: u64,
    /// Wall-clock instant at which the previous `spin_once` exited. The
    /// timer delta on the next call is measured from this point so any
    /// time the caller spent between `spin_once` invocations (e.g. an
    /// explicit `thread::sleep`) counts toward timer accumulation just
    /// like time spent inside `drive_io`.
    #[cfg(feature = "std")]
    pub(crate) last_spin_end: Option<std::time::Instant>,
}

impl Executor {
    /// Create an executor from an already-opened session.
    pub fn from_session(session: session::ConcreteSession) -> Self {
        // SAFETY: MaybeUninit::uninit() is always safe; these bytes are only
        // accessed through properly-typed ptr::write / ptr::read via the
        // dispatch function pointers stored in `entries`.
        Self {
            session: SessionStore::Owned(session),
            arena: [MaybeUninit::uninit(); crate::config::ARENA_SIZE],
            arena_used: 0,
            entries: [None; crate::config::MAX_CBS],
            sched_contexts: {
                let mut s = [None; crate::config::MAX_SC];
                s[0] = Some(super::sched_context::SchedContext::default());
                s
            },
            sched_context_bindings: [super::sched_context::SchedContextId(0);
                crate::config::MAX_CBS],
            sporadic_states: [None; crate::config::MAX_SC],
            #[cfg(feature = "alloc")]
            sporadic_atomic_states: [const { None }; crate::config::MAX_SC],
            major_frame_us: 0,
            trigger: Trigger::Any,
            semantics: ExecutorSemantics::RclcppExecutor,
            node_name: heapless::String::new(),
            namespace: {
                let mut ns = heapless::String::new();
                let _ = ns.push_str("/");
                ns
            },
            #[cfg(feature = "std")]
            halt_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            #[cfg(feature = "param-services")]
            params: None,
            #[cfg(feature = "lifecycle-services")]
            lifecycle: None,
            #[cfg(feature = "std")]
            spin_residual_us: 0,
            // Initialise the spin endpoint to construction time so the
            // very first `spin_once` credits time the caller spent
            // *before* it (e.g. setup, an explicit pre-spin sleep) just
            // like time spent between later calls.
            #[cfg(feature = "std")]
            last_spin_end: Some(std::time::Instant::now()),
        }
    }

    /// Create an executor from a borrowed session pointer.
    ///
    /// # Safety
    /// - `session_ptr` must point to a valid, initialized session that lives at
    ///   least as long as this executor.
    /// - The caller must not move or drop the session while the executor exists.
    pub unsafe fn from_session_ptr(session_ptr: *mut session::ConcreteSession) -> Self {
        Self {
            session: SessionStore::Borrowed(session_ptr),
            arena: [MaybeUninit::uninit(); crate::config::ARENA_SIZE],
            arena_used: 0,
            entries: [None; crate::config::MAX_CBS],
            sched_contexts: {
                let mut s = [None; crate::config::MAX_SC];
                s[0] = Some(super::sched_context::SchedContext::default());
                s
            },
            sched_context_bindings: [super::sched_context::SchedContextId(0);
                crate::config::MAX_CBS],
            sporadic_states: [None; crate::config::MAX_SC],
            #[cfg(feature = "alloc")]
            sporadic_atomic_states: [const { None }; crate::config::MAX_SC],
            major_frame_us: 0,
            trigger: Trigger::Any,
            semantics: ExecutorSemantics::RclcppExecutor,
            node_name: heapless::String::new(),
            namespace: {
                let mut ns = heapless::String::new();
                let _ = ns.push_str("/");
                ns
            },
            #[cfg(feature = "std")]
            halt_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            #[cfg(feature = "param-services")]
            params: None,
            #[cfg(feature = "lifecycle-services")]
            lifecycle: None,
            #[cfg(feature = "std")]
            spin_residual_us: 0,
            // Initialise the spin endpoint to construction time so the
            // very first `spin_once` credits time the caller spent
            // *before* it (e.g. setup, an explicit pre-spin sleep) just
            // like time spent between later calls.
            #[cfg(feature = "std")]
            last_spin_end: Some(std::time::Instant::now()),
        }
    }

    /// Set the node name and namespace used for liveliness tokens.
    ///
    /// Called by `open()` to propagate config values. When `add_subscription`
    /// or `add_service` creates entities, these values are attached to the
    /// `TopicInfo`/`ServiceInfo` so the zenoh backend can declare liveliness.
    pub fn set_node_identity(&mut self, node_name: &str, namespace: &str) {
        self.node_name.clear();
        let _ = self.node_name.push_str(node_name);
        if !namespace.is_empty() {
            self.namespace.clear();
            let _ = self.namespace.push_str(namespace);
        }
    }

    // =========================================================================
    // Phase 110.B — SchedContext API
    // =========================================================================

    /// Identifier of the auto-created default `Fifo`-class scheduling
    /// context. Every callback registered without an explicit
    /// [`bind_handle_to_sched_context`] binds to this SC.
    pub fn default_sched_context_id(&self) -> super::sched_context::SchedContextId {
        super::sched_context::SchedContextId(0)
    }

    /// Register a new scheduling context. Returns a [`SchedContextId`]
    /// callers pass to [`bind_handle_to_sched_context`] to attach
    /// callbacks. Phase 110.B.
    pub fn create_sched_context(
        &mut self,
        sc: super::sched_context::SchedContext,
    ) -> Result<super::sched_context::SchedContextId, NodeError> {
        // Slot 0 is reserved for the default Fifo SC; search 1..MAX_SC.
        for (i, slot) in self.sched_contexts.iter_mut().enumerate().skip(1) {
            if slot.is_none() {
                *slot = Some(sc);
                // Phase 110.E — Sporadic-class SCs get a sibling
                // `SporadicState` entry that the spin_once dispatch
                // path consults each cycle to refill the budget at
                // period boundaries and skip dispatch when budget
                // is exhausted.
                if matches!(sc.class, super::sched_context::SchedClass::Sporadic) {
                    let budget = sc.budget_us.get().map(|nz| nz.get()).unwrap_or(u32::MAX);
                    let period = sc.period_us.get().map(|nz| nz.get()).unwrap_or(u32::MAX);
                    self.sporadic_states[i] =
                        Some(super::sched_context::SporadicState::new(budget, period));
                }
                return Ok(super::sched_context::SchedContextId(i as u8));
            }
        }
        Err(NodeError::NoSchedContextSlot)
    }

    /// Bind a registered callback to a scheduling context. The next
    /// `spin_once` cycle dispatches the callback through that SC's
    /// queue (FIFO bitmap or EDF heap). Phase 110.B.
    pub fn bind_handle_to_sched_context(
        &mut self,
        handle: HandleId,
        sc_id: super::sched_context::SchedContextId,
    ) -> Result<(), NodeError> {
        let i = handle.0;
        if i >= crate::config::MAX_CBS {
            return Err(NodeError::InvalidSchedContextBinding);
        }
        if self.entries[i].is_none() {
            return Err(NodeError::InvalidSchedContextBinding);
        }
        let sc_idx = sc_id.0 as usize;
        if sc_idx >= crate::config::MAX_SC || self.sched_contexts[sc_idx].is_none() {
            return Err(NodeError::InvalidSchedContextBinding);
        }
        self.sched_context_bindings[i] = sc_id;
        Ok(())
    }

    /// Phase 110.G — enable time-triggered dispatch by setting the
    /// executor's major-frame length. Once set, every `spin_once`
    /// cycle gates dispatch through each entry's bound SC's
    /// `tt_window_offset_us` / `tt_window_duration_us` fields:
    /// dispatch only fires when the current monotonic time falls
    /// inside the window `[off, off + duration) mod major_frame`.
    ///
    /// `major_frame_us = 0` disables the TT gate (default state).
    /// Setting a non-zero major frame after callbacks are already
    /// registered is allowed — TT gates take effect on the next
    /// `spin_once` cycle.
    pub fn register_time_triggered_dispatcher(&mut self, major_frame_us: u32) {
        self.major_frame_us = major_frame_us;
    }

    /// Phase 110.E.b — register an ISR-driven refill timer for an
    /// already-created Sporadic SC. The caller invokes their
    /// platform's `PlatformTimer::create_periodic` with the returned
    /// `Arc<AtomicSporadicState>` as `user_data` and the
    /// `atomic_sporadic_refill_thunk` as the callback, then hands
    /// the resulting platform handle to this method via
    /// `OpaqueTimerHandle::new(handle, destroy_fn)`.
    ///
    /// The Executor stores both the Arc and the handle so Drop can
    /// clean them up. Calling this on a non-Sporadic SC returns
    /// `Err(InvalidSchedContextBinding)`.
    #[cfg(feature = "alloc")]
    pub fn register_sporadic_timer(
        &mut self,
        sc_id: super::sched_context::SchedContextId,
        timer: OpaqueTimerHandle,
    ) -> Result<alloc::sync::Arc<super::sched_context::AtomicSporadicState>, NodeError> {
        let i = sc_id.0 as usize;
        if i >= crate::config::MAX_SC {
            return Err(NodeError::InvalidSchedContextBinding);
        }
        let sc = self.sched_contexts[i]
            .as_ref()
            .ok_or(NodeError::InvalidSchedContextBinding)?;
        if !matches!(sc.class, super::sched_context::SchedClass::Sporadic) {
            return Err(NodeError::InvalidSchedContextBinding);
        }
        let budget = sc.budget_us.get().map(|nz| nz.get()).unwrap_or(u32::MAX);
        let period = sc.period_us.get().map(|nz| nz.get()).unwrap_or(u32::MAX);
        let state = alloc::sync::Arc::new(
            super::sched_context::AtomicSporadicState::new(budget, period),
        );
        self.sporadic_atomic_states[i] = Some((alloc::sync::Arc::clone(&state), timer));
        Ok(state)
    }

    /// Inspect a registered scheduling context. Phase 110.B.
    pub fn sched_context(
        &self,
        sc_id: super::sched_context::SchedContextId,
    ) -> Option<&super::sched_context::SchedContext> {
        self.sched_contexts.get(sc_id.0 as usize)?.as_ref()
    }

    /// Create a node on this executor.
    pub fn create_node(&mut self, name: &str) -> Result<Node<'_>, NodeError> {
        if name.len() > 64 {
            return Err(NodeError::NameTooLong);
        }

        let mut node_name = heapless::String::<64>::new();
        node_name
            .push_str(name)
            .map_err(|_| NodeError::NameTooLong)?;

        Ok(Node::new(
            node_name,
            self.namespace.clone(),
            &mut self.session,
            0,
        ))
    }

    /// Drive transport I/O (poll network, dispatch callbacks).
    #[allow(dead_code)]
    pub(crate) fn drive_io(&mut self, timeout_ms: i32) -> Result<(), NodeError> {
        self.session
            .drive_io(timeout_ms)
            .map_err(|_| NodeError::Transport(TransportError::PollFailed))
    }

    /// Close the underlying session.
    pub fn close(&mut self) -> Result<(), NodeError> {
        self.session
            .close()
            .map_err(|_| NodeError::Transport(TransportError::ConnectionFailed))
    }

    /// Get a reference to the underlying session.
    pub fn session(&self) -> &session::ConcreteSession {
        &self.session
    }

    /// Get a mutable reference to the underlying session.
    pub fn session_mut(&mut self) -> &mut session::ConcreteSession {
        &mut self.session
    }

    /// Get a mutable reference to an action client core in the arena by entry index.
    ///
    /// # Safety
    /// The caller must ensure that `entry_index` refers to an `ActionClientRawArenaEntry`.
    pub unsafe fn action_client_core_mut(
        &mut self,
        entry_index: usize,
    ) -> Option<&mut super::action_core::ActionClientCore> {
        let meta = self.entries.get(entry_index)?.as_ref()?;
        if !matches!(meta.kind, EntryKind::ActionClient) {
            return None;
        }
        let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
        unsafe {
            let entry_ptr = arena_ptr.add(meta.offset)
                as *mut super::arena::ActionClientRawArenaEntry<
                    { crate::config::DEFAULT_RX_BUF_SIZE },
                    { crate::config::DEFAULT_RX_BUF_SIZE },
                    { crate::config::DEFAULT_RX_BUF_SIZE },
                >;
            Some(&mut (*entry_ptr).core)
        }
    }

    /// Get a mutable reference to a service-client arena entry (Phase 82).
    ///
    /// Returns `None` if `entry_index` doesn't refer to a service client
    /// entry. The default reply buffer size is assumed because the C API
    /// always uses the default — the entry was registered via
    /// `add_service_client_raw_sized::<DEFAULT_RX_BUF_SIZE>`.
    ///
    /// # Safety
    /// `entry_index` must refer to a `ServiceClientRawArenaEntry`.
    pub unsafe fn service_client_entry_mut(
        &mut self,
        entry_index: usize,
    ) -> Option<&mut super::arena::ServiceClientRawArenaEntry<{ crate::config::DEFAULT_RX_BUF_SIZE }>>
    {
        let meta = self.entries.get(entry_index)?.as_ref()?;
        if !matches!(meta.kind, EntryKind::ServiceClient) {
            return None;
        }
        let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
        unsafe {
            let entry_ptr = arena_ptr.add(meta.offset)
                as *mut super::arena::ServiceClientRawArenaEntry<
                    { crate::config::DEFAULT_RX_BUF_SIZE },
                >;
            Some(&mut *entry_ptr)
        }
    }

    /// Set the executor-level trigger condition.
    ///
    /// Controls which handles must be ready before `spin_once` dispatches
    /// callbacks. Defaults to [`Trigger::AnyReady`](crate::Trigger).
    pub fn set_trigger(&mut self, trigger: Trigger) {
        self.trigger = trigger;
    }

    /// Set the executor data communication semantics.
    ///
    /// Choose between `Direct` (process in place) and `LET`
    /// (snapshot-then-process) semantics. See [`ExecutorSemantics`].
    pub fn set_semantics(&mut self, semantics: ExecutorSemantics) {
        self.semantics = semantics;
    }

    /// Set the invocation mode for a specific handle.
    ///
    /// Controls whether the callback fires on every spin
    /// ([`Always`](InvocationMode::Always)) or only when new data
    /// arrives ([`OnNewData`](InvocationMode::OnNewData), the default).
    pub fn set_invocation(&mut self, id: HandleId, mode: InvocationMode) {
        if let Some(Some(meta)) = self.entries.get_mut(id.0) {
            meta.invocation = mode;
        }
    }

    // ========================================================================
    // Arena-based callback registration
    // ========================================================================

    /// Bump-allocate space for `T` in the arena. Returns the byte offset.
    pub(crate) fn arena_alloc<T>(&mut self) -> Result<usize, NodeError> {
        let align = core::mem::align_of::<T>();
        let size = core::mem::size_of::<T>();
        let aligned_offset = (self.arena_used + align - 1) & !(align - 1);
        let new_used = aligned_offset + size;
        if new_used > crate::config::ARENA_SIZE {
            return Err(NodeError::BufferTooSmall);
        }
        self.arena_used = new_used;
        Ok(aligned_offset)
    }

    /// Bump-allocate space for `T` plus `trailing_bytes` extra bytes.
    ///
    /// Returns `(entry_offset, trailing_offset)`. The trailing region starts
    /// immediately after `T` (aligned to 8 bytes).
    pub(crate) fn arena_alloc_with_trailing<T>(
        &mut self,
        trailing_bytes: usize,
    ) -> Result<(usize, usize), NodeError> {
        let align = core::mem::align_of::<T>();
        let entry_size = core::mem::size_of::<T>();
        let entry_offset = (self.arena_used + align - 1) & !(align - 1);
        // Trailing region is 8-byte aligned after the entry struct
        let trailing_offset = (entry_offset + entry_size + 7) & !7;
        let new_used = trailing_offset + trailing_bytes;
        if new_used > crate::config::ARENA_SIZE {
            return Err(NodeError::BufferTooSmall);
        }
        self.arena_used = new_used;
        Ok((entry_offset, trailing_offset))
    }

    /// Find the next free entry slot index.
    pub(crate) fn next_entry_slot(&self) -> Result<usize, NodeError> {
        self.entries
            .iter()
            .position(|e| e.is_none())
            .ok_or(NodeError::BufferTooSmall)
    }

    /// Register a subscription callback with the default receive buffer size.
    ///
    /// The callback is stored in the arena and invoked during [`spin_once()`](Self::spin_once).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut executor = Executor::open(&config)?;
    /// executor.add_subscription::<Int32, _>("/chatter", |msg: &Int32| {
    ///     // handle message
    /// })?;
    /// loop {
    ///     executor.spin_once(core::time::Duration::from_millis(10));
    /// }
    /// ```
    pub fn add_subscription<M, F>(
        &mut self,
        topic_name: &str,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        M: RosMessage + 'static,
        F: FnMut(&M) + 'static,
    {
        self.add_subscription_sized::<M, F, { crate::config::DEFAULT_RX_BUF_SIZE }>(
            topic_name, callback,
        )
    }

    /// Register a subscription callback with a custom receive buffer size.
    ///
    /// Internally uses a triple buffer (3 slots) with `KEEP_LAST(1)` QoS.
    /// For deeper message queuing, use [`add_subscription_buffered`] with
    /// an explicit QoS depth.
    pub fn add_subscription_sized<M, F, const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        M: RosMessage + 'static,
        F: FnMut(&M) + 'static,
    {
        // Use depth=1 (triple buffer) to match the old single-buffer behavior.
        // The default QoS depth (10) would create an 11-slot SPSC ring, using
        // 11× the buffer memory — too expensive as an invisible default.
        self.add_subscription_buffered::<M, F, RX_BUF>(
            topic_name,
            QosSettings::default().keep_last(1),
            callback,
        )
    }

    /// Register a subscription with QoS-driven buffering (Phase 73).
    ///
    /// The buffer strategy is selected by the QoS depth:
    /// - `KEEP_LAST(1)` → triple buffer (3 slots, latest-value, no message loss)
    /// - `KEEP_LAST(N)` where N > 1 → SPSC ring (N+1 slots, FIFO, bounded drops)
    ///
    /// Buffer slots are allocated as a trailing region in the arena (no
    /// separate static buffer array). `RX_BUF` sets the per-slot byte size.
    pub fn add_subscription_buffered<M, F, const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
        qos: QosSettings,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        M: RosMessage + 'static,
        F: FnMut(&M) + 'static,
    {
        type Entry<M, F> = SubBufferedEntry<M, F>;

        let slot = self.next_entry_slot()?;
        let node_name: heapless::String<64> = self.node_name.clone();
        let ns: heapless::String<64> = self.namespace.clone();
        let mut topic = TopicInfo::new(topic_name, M::TYPE_NAME, M::TYPE_HASH).with_namespace(&ns);
        if !node_name.is_empty() {
            topic = topic.with_node_name(&node_name);
        }
        let handle = self
            .session
            .create_subscriber(&topic, qos)
            .map_err(|_| NodeError::Transport(TransportError::SubscriberCreationFailed))?;

        let (_slot_count, trailing_bytes) = buffered_region_size(qos.depth, RX_BUF);

        let (entry_offset, trailing_offset) =
            self.arena_alloc_with_trailing::<Entry<M, F>>(trailing_bytes)?;

        let buf_ptr = unsafe { (self.arena.as_mut_ptr() as *mut u8).add(trailing_offset) };

        let buffer = if qos.depth <= 1 {
            BufferStrategy::Triple(unsafe { TripleBuffer::init(buf_ptr, RX_BUF) })
        } else {
            BufferStrategy::Ring(unsafe { SpscRing::init(buf_ptr, RX_BUF, qos.depth as usize) })
        };

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(entry_offset) as *mut Entry<M, F>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    handle,
                    buffer,
                    callback,
                    _phantom: PhantomData,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset: entry_offset,
            kind: EntryKind::Subscription,
            try_process: sub_buffered_try_process::<M, F>,
            has_data: sub_buffered_has_data::<M, F>,
            pre_sample: no_pre_sample, // LET pre-sample not yet supported for buffered
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<Entry<M, F>>,
        });
        Ok(HandleId(slot))
    }

    /// Register a zero-copy raw subscription with QoS-driven buffering.
    ///
    /// The callback receives `&[u8]` — the raw CDR data borrowing directly
    /// from the triple buffer's read slot or SPSC ring's pop slot. For
    /// borrowed message types (e.g., `Image<'a>`), call
    /// `Image::deserialize_borrowed(data)` inside the callback:
    ///
    /// ```ignore
    /// executor.add_subscription_buffered_raw::<1024>(
    ///     "/camera/image",
    ///     "sensor_msgs::msg::dds_::Image_",
    ///     "TypeHashNotSupported",
    ///     QosSettings::SENSOR_DATA,
    ///     |data: &[u8]| {
    ///         let img = Image::deserialize_borrowed(data).unwrap();
    ///         process_pixels(img.data); // img.data: &[u8] borrowing from `data`
    ///     },
    /// );
    /// ```
    pub fn add_subscription_buffered_raw<F, const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QosSettings,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        F: FnMut(&[u8]) + 'static,
    {
        let node_name: heapless::String<64> = self.node_name.clone();
        let ns: heapless::String<64> = self.namespace.clone();
        let mut topic = TopicInfo::new(topic_name, type_name, type_hash).with_namespace(&ns);
        if !node_name.is_empty() {
            topic = topic.with_node_name(&node_name);
        }
        let handle = self
            .session
            .create_subscriber(&topic, qos)
            .map_err(|_| NodeError::Transport(TransportError::SubscriberCreationFailed))?;
        self.add_arena_subscription_callback::<F, RX_BUF>(handle, qos, callback)
    }

    /// Register a raw byte-shaped callback against a pre-built
    /// `RmwSubscriber` handle.
    ///
    /// Backend-agnostic primitive — the caller is responsible for
    /// obtaining the handle by whatever route the active backend
    /// supports:
    ///
    /// - **Generic ROS-typed flow**: call `Session::create_subscriber`
    ///   on `self.session_mut()` with a [`TopicInfo`].
    ///   [`add_subscription_buffered_raw`](Self::add_subscription_buffered_raw)
    ///   is the convenience wrapper for this path.
    /// - **Backend-specific flow** (e.g. uORB needs `&'static orb_metadata`):
    ///   reach into the concrete session via [`Self::session_mut`] and
    ///   call its backend-specific create method, then hand the handle
    ///   here. `nros-px4::uorb::create_subscription_with_callback` is
    ///   the example.
    ///
    /// The arena-store + vtable wiring is identical to
    /// `add_subscription_buffered_raw`; the only thing that varies is
    /// where the handle came from. Callback fires on every message
    /// delivery during [`spin_once`](Self::spin_once); bytes are
    /// passed as `&[u8]`.
    pub fn add_arena_subscription_callback<F, const RX_BUF: usize>(
        &mut self,
        handle: session::RmwSubscriber,
        qos: QosSettings,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        F: FnMut(&[u8]) + 'static,
    {
        type Entry<F> = SubBufferedRawEntry<F>;

        let slot = self.next_entry_slot()?;
        let (_slot_count, trailing_bytes) = buffered_region_size(qos.depth, RX_BUF);

        let (entry_offset, trailing_offset) =
            self.arena_alloc_with_trailing::<Entry<F>>(trailing_bytes)?;

        let buf_ptr = unsafe { (self.arena.as_mut_ptr() as *mut u8).add(trailing_offset) };

        let buffer = if qos.depth <= 1 {
            BufferStrategy::Triple(unsafe { TripleBuffer::init(buf_ptr, RX_BUF) })
        } else {
            BufferStrategy::Ring(unsafe { SpscRing::init(buf_ptr, RX_BUF, qos.depth as usize) })
        };

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(entry_offset) as *mut Entry<F>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    handle,
                    buffer,
                    callback,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset: entry_offset,
            kind: EntryKind::Subscription,
            try_process: sub_buffered_raw_try_process::<F>,
            has_data: sub_buffered_raw_has_data::<F>,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<Entry<F>>,
        });
        Ok(HandleId(slot))
    }

    /// Register a subscription callback that receives both the message and
    /// [`MessageInfo`](nros_core::MessageInfo) (sequence number, publisher GID, timestamps).
    ///
    /// The callback is stored in the arena and invoked during [`spin_once()`](Self::spin_once).
    ///
    /// # Example
    ///
    /// ```ignore
    /// executor.add_subscription_with_info::<Int32, _>("/chatter", |msg, info| {
    ///     if let Some(info) = info {
    ///         log::trace!("seq={} gid={:02x?}", info.publication_sequence_number(), &info.publisher_gid()[..4]);
    ///     }
    /// })?;
    /// ```
    pub fn add_subscription_with_info<M, F>(
        &mut self,
        topic_name: &str,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        M: RosMessage + 'static,
        F: FnMut(&M, Option<&nros_core::MessageInfo>) + 'static,
    {
        self.add_subscription_with_info_sized::<M, F, { crate::config::DEFAULT_RX_BUF_SIZE }>(
            topic_name, callback,
        )
    }

    /// Register a subscription callback with MessageInfo and a custom receive buffer size.
    pub fn add_subscription_with_info_sized<M, F, const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        M: RosMessage + 'static,
        F: FnMut(&M, Option<&nros_core::MessageInfo>) + 'static,
    {
        type Entry<M, F, const N: usize> = SubInfoEntry<M, F, N>;

        let slot = self.next_entry_slot()?;
        let node_name: heapless::String<64> = self.node_name.clone();
        let ns: heapless::String<64> = self.namespace.clone();
        let mut topic = TopicInfo::new(topic_name, M::TYPE_NAME, M::TYPE_HASH).with_namespace(&ns);
        if !node_name.is_empty() {
            topic = topic.with_node_name(&node_name);
        }
        let handle = self
            .session
            .create_subscriber(&topic, QosSettings::default())
            .map_err(|_| NodeError::Transport(TransportError::SubscriberCreationFailed))?;

        let offset = self.arena_alloc::<Entry<M, F, RX_BUF>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut Entry<M, F, RX_BUF>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    handle,
                    buffer: [0u8; RX_BUF],
                    sampled_len: 0,
                    callback,
                    _phantom: PhantomData,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Subscription,
            try_process: sub_info_try_process::<M, F, RX_BUF>,
            has_data: sub_info_has_data::<M, F, RX_BUF>,
            pre_sample: sub_info_pre_sample::<M, F, RX_BUF>,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<Entry<M, F, RX_BUF>>,
        });
        Ok(HandleId(slot))
    }

    /// Register a subscription callback with E2E safety validation (CRC + sequence tracking).
    ///
    /// The callback receives the deserialized message and an [`IntegrityStatus`](nros_rmw::IntegrityStatus)
    /// with CRC validation results and sequence gap/duplicate detection.
    ///
    /// # Example
    ///
    /// ```ignore
    /// executor.add_subscription_with_safety::<Int32, _>("/chatter", |msg, status| {
    ///     let crc_str = match status.crc_valid {
    ///         Some(true) => "ok",
    ///         Some(false) => "FAIL",
    ///         None => "n/a",
    ///     };
    ///     println!("[SAFETY] seq_gap={} dup={} crc={}", status.gap, status.duplicate, crc_str);
    /// })?;
    /// ```
    #[cfg(feature = "safety-e2e")]
    pub fn add_subscription_with_safety<M, F>(
        &mut self,
        topic_name: &str,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        M: RosMessage + 'static,
        F: FnMut(&M, &nros_rmw::IntegrityStatus) + 'static,
    {
        self.add_subscription_with_safety_sized::<M, F, { crate::config::DEFAULT_RX_BUF_SIZE }>(
            topic_name, callback,
        )
    }

    /// Register a safety-validated subscription callback with a custom receive buffer size.
    #[cfg(feature = "safety-e2e")]
    pub fn add_subscription_with_safety_sized<M, F, const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        M: RosMessage + 'static,
        F: FnMut(&M, &nros_rmw::IntegrityStatus) + 'static,
    {
        type Entry<M, F, const N: usize> = SubSafetyEntry<M, F, N>;

        let slot = self.next_entry_slot()?;
        let node_name: heapless::String<64> = self.node_name.clone();
        let ns: heapless::String<64> = self.namespace.clone();
        let mut topic = TopicInfo::new(topic_name, M::TYPE_NAME, M::TYPE_HASH).with_namespace(&ns);
        if !node_name.is_empty() {
            topic = topic.with_node_name(&node_name);
        }
        let handle = self
            .session
            .create_subscriber(&topic, QosSettings::default())
            .map_err(|_| NodeError::Transport(TransportError::SubscriberCreationFailed))?;

        let offset = self.arena_alloc::<Entry<M, F, RX_BUF>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut Entry<M, F, RX_BUF>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    handle,
                    buffer: [0u8; RX_BUF],
                    sampled_len: 0,
                    callback,
                    _phantom: PhantomData,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Subscription,
            try_process: sub_safety_try_process::<M, F, RX_BUF>,
            has_data: sub_safety_has_data::<M, F, RX_BUF>,
            pre_sample: sub_safety_pre_sample::<M, F, RX_BUF>,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<Entry<M, F, RX_BUF>>,
        });
        Ok(HandleId(slot))
    }

    /// Register a service callback with the default buffer size.
    ///
    /// The callback is stored in the arena and invoked during [`spin_once()`](Self::spin_once).
    pub fn add_service<Svc, F>(
        &mut self,
        service_name: &str,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        Svc: RosService + 'static,
        F: FnMut(&Svc::Request) -> Svc::Reply + 'static,
    {
        self.add_service_sized::<Svc, F, { crate::config::DEFAULT_RX_BUF_SIZE }, { crate::config::DEFAULT_RX_BUF_SIZE }>(service_name, callback)
    }

    /// Register a service callback with custom request/reply buffer sizes.
    pub fn add_service_sized<Svc, F, const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        Svc: RosService + 'static,
        F: FnMut(&Svc::Request) -> Svc::Reply + 'static,
    {
        type Entry<Svc, F, const RQ: usize, const RP: usize> = SrvEntry<Svc, F, RQ, RP>;

        let slot = self.next_entry_slot()?;
        let node_name: heapless::String<64> = self.node_name.clone();
        let ns: heapless::String<64> = self.namespace.clone();
        let mut info = ServiceInfo::new(service_name, Svc::SERVICE_NAME, Svc::SERVICE_HASH)
            .with_namespace(&ns);
        if !node_name.is_empty() {
            info = info.with_node_name(&node_name);
        }
        let handle = self
            .session
            .create_service_server(&info)
            .map_err(|_| NodeError::Transport(TransportError::ServiceServerCreationFailed))?;

        let offset = self.arena_alloc::<Entry<Svc, F, REQ_BUF, REPLY_BUF>>()?;

        // SAFETY: same guarantees as add_subscription_sized.
        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut Entry<Svc, F, REQ_BUF, REPLY_BUF>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    handle,
                    req_buffer: [0u8; REQ_BUF],
                    reply_buffer: [0u8; REPLY_BUF],
                    callback,
                    _phantom: PhantomData,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Service,
            try_process: srv_try_process::<Svc, F, REQ_BUF, REPLY_BUF>,
            has_data: srv_has_data::<Svc, F, REQ_BUF, REPLY_BUF>,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<Entry<Svc, F, REQ_BUF, REPLY_BUF>>,
        });
        Ok(HandleId(slot))
    }

    // ========================================================================
    // Timer registration
    // ========================================================================

    /// Register a repeating timer callback.
    ///
    /// The callback fires every `period` milliseconds during [`spin_once()`](Self::spin_once).
    /// The timer delta is approximated by the `timeout_ms` argument to `spin_once`.
    pub fn add_timer<F>(
        &mut self,
        period: TimerDuration,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        F: FnMut() + 'static,
    {
        let slot = self.next_entry_slot()?;
        let offset = self.arena_alloc::<TimerEntry<F>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut TimerEntry<F>;
            core::ptr::write(
                entry_ptr,
                TimerEntry {
                    period_ms: period.as_millis(),
                    elapsed_ms: 0,
                    oneshot: false,
                    fired: false,
                    cancelled: false,
                    callback,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Timer,
            try_process: timer_try_process::<F>,
            has_data: always_ready,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::Always,
            drop_fn: drop_entry::<TimerEntry<F>>,
        });
        Ok(HandleId(slot))
    }

    /// Register a one-shot timer callback.
    ///
    /// The callback fires once after `delay` milliseconds, then becomes inert.
    pub fn add_timer_oneshot<F>(
        &mut self,
        delay: TimerDuration,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        F: FnMut() + 'static,
    {
        let slot = self.next_entry_slot()?;
        let offset = self.arena_alloc::<TimerEntry<F>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut TimerEntry<F>;
            core::ptr::write(
                entry_ptr,
                TimerEntry {
                    period_ms: delay.as_millis(),
                    elapsed_ms: 0,
                    oneshot: true,
                    fired: false,
                    cancelled: false,
                    callback,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Timer,
            try_process: timer_try_process::<F>,
            has_data: always_ready,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::Always,
            drop_fn: drop_entry::<TimerEntry<F>>,
        });
        Ok(HandleId(slot))
    }

    // ========================================================================
    // Raw callback registration (for C API)
    // ========================================================================

    /// Register a raw (untyped) subscription callback with default QoS.
    ///
    /// The callback receives CDR bytes without deserialization.
    /// Used by the C API where generic type parameters are not available.
    pub fn add_subscription_raw(
        &mut self,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        callback: RawSubscriptionCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        self.add_subscription_raw_with_qos_sized::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
            topic_name,
            type_name,
            type_hash,
            QosSettings::default().keep_last(1),
            callback,
            context,
        )
    }

    /// Register a raw subscription callback with a custom receive buffer size.
    pub fn add_subscription_raw_sized<const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        callback: RawSubscriptionCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        self.add_subscription_raw_with_qos_sized::<RX_BUF>(
            topic_name,
            type_name,
            type_hash,
            QosSettings::default().keep_last(1),
            callback,
            context,
        )
    }

    /// Register a raw (untyped) subscription callback with custom QoS.
    ///
    /// Used by the C API where QoS is specified at init time.
    pub fn add_subscription_raw_with_qos(
        &mut self,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QosSettings,
        callback: RawSubscriptionCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        self.add_subscription_raw_with_qos_sized::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
            topic_name, type_name, type_hash, qos, callback, context,
        )
    }

    /// Register a raw subscription callback with custom QoS and buffer size.
    ///
    /// Internally uses triple buffer (depth ≤ 1) or SPSC ring (depth > 1).
    pub fn add_subscription_raw_with_qos_sized<const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QosSettings,
        callback: RawSubscriptionCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        let slot = self.next_entry_slot()?;
        let node_name: heapless::String<64> = self.node_name.clone();
        let ns: heapless::String<64> = self.namespace.clone();
        let mut topic = TopicInfo::new(topic_name, type_name, type_hash).with_namespace(&ns);
        if !node_name.is_empty() {
            topic = topic.with_node_name(&node_name);
        }
        let handle = self
            .session
            .create_subscriber(&topic, qos)
            .map_err(|_| NodeError::Transport(TransportError::SubscriberCreationFailed))?;

        let (_slot_count, trailing_bytes) = buffered_region_size(qos.depth, RX_BUF);

        let (entry_offset, trailing_offset) =
            self.arena_alloc_with_trailing::<SubBufferedRawCEntry>(trailing_bytes)?;

        let buf_ptr = unsafe { (self.arena.as_mut_ptr() as *mut u8).add(trailing_offset) };

        let buffer = if qos.depth <= 1 {
            BufferStrategy::Triple(unsafe { TripleBuffer::init(buf_ptr, RX_BUF) })
        } else {
            BufferStrategy::Ring(unsafe { SpscRing::init(buf_ptr, RX_BUF, qos.depth as usize) })
        };

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(entry_offset) as *mut SubBufferedRawCEntry;
            core::ptr::write(
                entry_ptr,
                SubBufferedRawCEntry {
                    handle,
                    buffer,
                    callback,
                    context,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset: entry_offset,
            kind: EntryKind::Subscription,
            try_process: sub_buffered_raw_c_try_process,
            has_data: sub_buffered_raw_c_has_data,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<SubBufferedRawCEntry>,
        });
        Ok(HandleId(slot))
    }

    /// Register a raw (untyped) service callback.
    ///
    /// Register a raw (untyped) service callback with the default buffer size.
    ///
    /// The callback receives and produces CDR bytes without typed
    /// deserialization/serialization. Used by the C API wrapper.
    pub fn add_service_raw(
        &mut self,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        callback: RawServiceCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        self.add_service_raw_sized::<{ crate::config::DEFAULT_RX_BUF_SIZE }, { crate::config::DEFAULT_RX_BUF_SIZE }>(
            service_name,
            service_type,
            service_hash,
            callback,
            context,
        )
    }

    /// Register a raw (untyped) service callback with custom buffer sizes.
    ///
    /// `REQ_BUF` and `REPLY_BUF` set the stack-allocated CDR buffers
    /// for the request and reply respectively. Increase for services
    /// with large payloads (e.g., parameter services).
    pub fn add_service_raw_sized<const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        callback: RawServiceCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        let slot = self.next_entry_slot()?;
        let node_name: heapless::String<64> = self.node_name.clone();
        let ns: heapless::String<64> = self.namespace.clone();
        let mut info =
            ServiceInfo::new(service_name, service_type, service_hash).with_namespace(&ns);
        if !node_name.is_empty() {
            info = info.with_node_name(&node_name);
        }
        let handle = self
            .session
            .create_service_server(&info)
            .map_err(|_| NodeError::Transport(TransportError::ServiceServerCreationFailed))?;

        let offset = self.arena_alloc::<SrvRawEntry<REQ_BUF, REPLY_BUF>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut SrvRawEntry<REQ_BUF, REPLY_BUF>;
            core::ptr::write(
                entry_ptr,
                SrvRawEntry {
                    handle,
                    req_buffer: [0u8; REQ_BUF],
                    reply_buffer: [0u8; REPLY_BUF],
                    callback,
                    context,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Service,
            try_process: srv_raw_try_process::<REQ_BUF, REPLY_BUF>,
            has_data: srv_raw_has_data::<REQ_BUF, REPLY_BUF>,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<SrvRawEntry<REQ_BUF, REPLY_BUF>>,
        });
        Ok(HandleId(slot))
    }

    // ========================================================================
    // Raw service client registration (Phase 82)
    // ========================================================================

    /// Register a raw (untyped) service client with the default reply
    /// buffer size.
    ///
    /// The client is owned by the executor's arena. Each `spin_once`
    /// dispatch polls the in-flight reply slot via `try_recv_reply_raw`
    /// and fires the registered callback when the response arrives.
    /// Used by the C API thin wrapper — see Phase 82.
    pub fn add_service_client_raw(
        &mut self,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        callback: Option<RawResponseCallback>,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        self.add_service_client_raw_sized::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
            service_name,
            service_type,
            service_hash,
            callback,
            context,
        )
    }

    /// Register a raw service client with a custom reply buffer size.
    pub fn add_service_client_raw_sized<const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        callback: Option<RawResponseCallback>,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        let slot = self.next_entry_slot()?;
        let node_name: heapless::String<64> = self.node_name.clone();
        let ns: heapless::String<64> = self.namespace.clone();
        let mut info =
            ServiceInfo::new(service_name, service_type, service_hash).with_namespace(&ns);
        if !node_name.is_empty() {
            info = info.with_node_name(&node_name);
        }
        let handle = self
            .session
            .create_service_client(&info)
            .map_err(|_| NodeError::Transport(TransportError::ServiceClientCreationFailed))?;

        let offset = self.arena_alloc::<ServiceClientRawArenaEntry<REPLY_BUF>>()?;
        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut ServiceClientRawArenaEntry<REPLY_BUF>;
            core::ptr::write(
                entry_ptr,
                ServiceClientRawArenaEntry {
                    handle,
                    reply_buffer: [0u8; REPLY_BUF],
                    pending: false,
                    reply_ready: core::sync::atomic::AtomicBool::new(false),
                    callback,
                    context,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::ServiceClient,
            try_process: service_client_raw_try_process::<REPLY_BUF>,
            has_data: always_ready,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::Always,
            drop_fn: drop_entry::<ServiceClientRawArenaEntry<REPLY_BUF>>,
        });
        Ok(HandleId(slot))
    }

    // ========================================================================
    // Guard condition registration
    // ========================================================================

    /// Register a guard condition with a callback.
    ///
    /// Returns both the [`HandleId`] for trigger configuration and a
    /// [`GuardConditionHandle`] for triggering from other threads.
    pub fn add_guard_condition<F>(
        &mut self,
        callback: F,
    ) -> Result<(HandleId, GuardConditionHandle), NodeError>
    where
        F: FnMut() + 'static,
    {
        let slot = self.next_entry_slot()?;
        let offset = self.arena_alloc::<GuardConditionEntry<F>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut GuardConditionEntry<F>;
            core::ptr::write(
                entry_ptr,
                GuardConditionEntry {
                    flag: portable_atomic::AtomicBool::new(false),
                    callback,
                },
            );

            // Create a handle pointing to the flag in the arena
            let flag_ptr = &(*entry_ptr).flag as *const portable_atomic::AtomicBool;
            let guard_handle = GuardConditionHandle::new(flag_ptr);

            self.entries[slot] = Some(CallbackMeta {
                offset,
                kind: EntryKind::GuardCondition,
                try_process: guard_try_process::<F>,
                has_data: guard_has_data::<F>,
                pre_sample: no_pre_sample,
                invocation: InvocationMode::OnNewData,
                drop_fn: drop_entry::<GuardConditionEntry<F>>,
            });

            Ok((HandleId(slot), guard_handle))
        }
    }

    // ========================================================================
    // Timer control methods
    // ========================================================================

    /// Cancel a timer. A cancelled timer will not fire but still accumulates
    /// elapsed time. The timer can be restarted with [`reset_timer()`](Self::reset_timer).
    pub fn cancel_timer(&mut self, id: HandleId) -> Result<(), NodeError> {
        let meta = self
            .entries
            .get(id.0)
            .and_then(|e| e.as_ref())
            .ok_or(NodeError::BufferTooSmall)?;
        if !matches!(meta.kind, EntryKind::Timer) {
            return Err(NodeError::BufferTooSmall);
        }
        let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
        // SAFETY: meta.offset points to a valid TimerEntry<F> which shares
        // layout with TimerHeader for its initial fields (both #[repr(C)]).
        let header = unsafe { &mut *(arena_ptr.add(meta.offset) as *mut TimerHeader) };
        header.cancelled = true;
        Ok(())
    }

    /// Reset a timer. Clears the cancelled state and resets the elapsed time
    /// to zero, so the timer starts a fresh period.
    pub fn reset_timer(&mut self, id: HandleId) -> Result<(), NodeError> {
        let meta = self
            .entries
            .get(id.0)
            .and_then(|e| e.as_ref())
            .ok_or(NodeError::BufferTooSmall)?;
        if !matches!(meta.kind, EntryKind::Timer) {
            return Err(NodeError::BufferTooSmall);
        }
        let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
        let header = unsafe { &mut *(arena_ptr.add(meta.offset) as *mut TimerHeader) };
        header.cancelled = false;
        header.elapsed_ms = 0;
        Ok(())
    }

    /// Check if a timer is cancelled.
    pub fn timer_is_cancelled(&self, id: HandleId) -> bool {
        let meta = match self.entries.get(id.0).and_then(|e| e.as_ref()) {
            Some(m) if matches!(m.kind, EntryKind::Timer) => m,
            _ => return false,
        };
        let arena_ptr = self.arena.as_ptr() as *const u8;
        let header = unsafe { &*(arena_ptr.add(meta.offset) as *const TimerHeader) };
        header.cancelled
    }

    /// Get the period of a timer in milliseconds, or `None` if the handle
    /// is not a valid timer.
    pub fn timer_period_ms(&self, id: HandleId) -> Option<u64> {
        let meta = self
            .entries
            .get(id.0)
            .and_then(|e| e.as_ref())
            .filter(|m| matches!(m.kind, EntryKind::Timer))?;
        let arena_ptr = self.arena.as_ptr() as *const u8;
        let header = unsafe { &*(arena_ptr.add(meta.offset) as *const TimerHeader) };
        Some(header.period_ms)
    }

    // ========================================================================
    // spin_once (three-phase: readiness -> trigger -> dispatch)
    // ========================================================================

    /// Drive I/O and dispatch registered callbacks once.
    ///
    /// Three-phase execution:
    /// 1. **Readiness scan** — query each handle's `has_data()`.
    /// 2. **Trigger evaluation** — check if the executor-level trigger passes.
    /// 3. **Dispatch** — invoke callbacks according to their `InvocationMode`.
    ///
    /// Returns a [`SpinOnceResult`] with counts of processed items and errors.
    ///
    /// # Arguments
    /// * `timeout` — upper bound on the I/O wait. Saturated at
    ///   `i32::MAX` ms (~24 days) for the underlying transport call.
    ///
    /// Phase 84.D7: unified on `core::time::Duration`. The previous
    /// `timeout_ms: i32` signature had a latent footgun where
    /// `spin_once(-1)` silently froze timers while still polling I/O;
    /// `Duration` has no negative sentinel.
    pub fn spin_once(&mut self, timeout: core::time::Duration) -> SpinOnceResult {
        let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;

        // Phase 110.0 — cap against the backend's next internal-event
        // deadline (lease keepalive, heartbeat, ACK-NACK timeout, ...).
        // Default backend impl returns `None`, so this is a no-op
        // unless the active backend opts in.
        let timeout_ms = match self.session.next_deadline_ms() {
            Some(next) => timeout_ms.min(next.min(i32::MAX as u32) as i32),
            None => timeout_ms,
        };

        // Wall-clock-accurate timer accumulation. Measure real time
        // since the previous `spin_once` exited (or, on the first call,
        // since `drive_io` started). Two failure modes the requested
        // `timeout_ms` doesn't capture:
        //  1. `drive_io` returns early — e.g. zenoh-pico's condvar wakes
        //     on data arrival, well under 1 ms.
        //  2. The caller spends time outside `spin_once` (explicit sleep,
        //     ROS-2 cooperative scheduling, etc.) and that time should
        //     still count toward timers.
        // Crediting the requested timeout to timers in either case ticks
        // them faster than wall-clock — observed as a 30 Hz control loop
        // overshooting to >200 Hz under sustained traffic. Carry the
        // sub-ms remainder across calls so precision is preserved.
        #[cfg(feature = "std")]
        let spin_start = std::time::Instant::now();

        let _ = self.session.drive_io(timeout_ms);

        #[cfg(feature = "std")]
        let delta_ms = {
            let now = std::time::Instant::now();
            // `last_spin_end` is seeded at construction time, so this
            // path always has a Some(_) on every call.
            let prev = self.last_spin_end.unwrap_or(spin_start);
            let elapsed = now.saturating_duration_since(prev);
            self.last_spin_end = Some(now);
            let total_us = self
                .spin_residual_us
                .saturating_add(elapsed.as_micros() as u64);
            let ms = total_us / 1000;
            self.spin_residual_us = total_us % 1000;
            ms
        };
        #[cfg(not(feature = "std"))]
        // TODO(no_std): same overshoot bug as the std path used to have —
        // `timeout_ms` is the requested upper bound, not the elapsed wall
        // clock. If a bare-metal `drive_io` returns early (e.g. zpico_spin_once
        // on a serial transport with non-blocking read), timers tick faster
        // than wall-clock. Fix is to thread `<P as nros_platform::PlatformClock>
        // ::clock_us()` through Executor (e.g. as a `clock_us_fn: fn() -> u64`
        // field set at construction). Deferred until a no_std workload
        // surfaces the bug — std workloads (autoware_sentinel) hit it first
        // and that path is now correct.
        let delta_ms = timeout_ms as u64;
        let arena_ptr = self.arena.as_mut_ptr() as *mut u8;

        // Phase 1: Readiness scan (Phase 110.A.b — backed by FifoReadySet).
        //
        // `bits` carries data-readiness only (used by trigger eval +
        // by `InvocationMode::OnNewData`). `always_mask` carries the
        // `InvocationMode::Always` entries that fire regardless of
        // data presence. The dispatcher drains
        // `FifoReadySet(bits | always_mask)` after the trigger
        // passes; `pop_next` yields registration order (lowest bit
        // first) so behavior is bit-identical to the pre-refactor
        // `for (i, meta) in entries.iter().enumerate()` loop.
        let mut bits: u64 = 0;
        let mut count: usize = 0;
        let mut non_timer_mask: u64 = 0;
        let mut always_mask: u64 = 0;

        for (i, meta) in self.entries.iter().enumerate() {
            if let Some(meta) = meta {
                let data_ptr = unsafe { arena_ptr.add(meta.offset) as *const u8 };
                if unsafe { (meta.has_data)(data_ptr) } {
                    bits |= 1u64 << i;
                }
                if !matches!(meta.kind, EntryKind::Timer | EntryKind::GuardCondition) {
                    non_timer_mask |= 1u64 << i;
                }
                if matches!(meta.invocation, InvocationMode::Always) {
                    always_mask |= 1u64 << i;
                }
                count += 1;
            }
        }

        let snapshot = ReadinessSnapshot { bits, count };

        // Phase 2: Trigger evaluation
        let trigger_passes = match &self.trigger {
            Trigger::Any => bits & non_timer_mask != 0 || non_timer_mask == 0,
            Trigger::All => bits & non_timer_mask == non_timer_mask,
            Trigger::One(id) => snapshot.is_ready(*id),
            Trigger::AllOf(set) => snapshot.all_ready(*set),
            Trigger::AnyOf(set) => snapshot.any_ready(*set),
            Trigger::Always => true,
            Trigger::Predicate(f) => f(&snapshot),
            Trigger::RawPredicate { callback, context } => {
                // Convert ReadinessSnapshot bitmask to a bool array for the C callback
                let mut ready_array = [false; 64];
                for (i, slot) in ready_array
                    .iter_mut()
                    .enumerate()
                    .take(snapshot.count.min(64))
                {
                    *slot = snapshot.bits & (1u64 << i) != 0;
                }
                // SAFETY: The callback and context are provided by the C API caller.
                // The ready_array is valid for snapshot.count elements.
                unsafe { callback(ready_array.as_ptr(), snapshot.count, *context) }
            }
        };

        if !trigger_passes {
            // Timers still need delta accumulation even when trigger doesn't pass
            for meta in self.entries.iter().flatten() {
                if matches!(meta.kind, EntryKind::Timer) {
                    let data_ptr = unsafe { arena_ptr.add(meta.offset) };
                    let _ = unsafe { (meta.try_process)(data_ptr, delta_ms) };
                }
            }

            // Parameter services live outside the arena and must be processed
            // regardless of trigger state, otherwise ROS 2 param queries time out.
            #[cfg(feature = "param-services")]
            if let Some(params) = &mut self.params {
                let crate::parameter_services::ParamState { server, services } = &mut **params;
                let _ = services.process_services(server);
            }

            // Same treatment for lifecycle services — `ros2 lifecycle get`
            // must succeed even when no callbacks fired this tick.
            // SAFETY: see the matching invariant on the later call site.
            #[cfg(feature = "lifecycle-services")]
            if let Some(lc) = &mut self.lifecycle {
                let crate::lifecycle_services::LifecycleRuntimeState {
                    state_machine,
                    services,
                } = &mut **lc;
                let _ = unsafe { services.process_services(state_machine) };
            }

            return SpinOnceResult::new();
        }

        // Phase 2.5: LET pre-sample (only when LogicalExecutionTime)
        //
        // Sample all subscription data into entry buffers BEFORE dispatching
        // any callbacks. This ensures all callbacks in this cycle see a
        // consistent snapshot of data from the same point in time.
        // Services are NOT pre-sampled (request-reply is sequential).
        if matches!(self.semantics, ExecutorSemantics::LogicalExecutionTime) {
            for meta in self.entries.iter().flatten() {
                if matches!(meta.kind, EntryKind::Subscription) {
                    let data_ptr = unsafe { arena_ptr.add(meta.offset) };
                    unsafe { (meta.pre_sample)(data_ptr) };
                }
            }
        }

        // Phase 3: Dispatch (Phase 110.C — bucketed by SC.priority).
        //
        // Two ready-set families, each split across `Priority::COUNT`
        // buckets (Critical / Normal / BestEffort). Per-entry SC
        // `class` selects FIFO bitmap vs EDF heap; SC `priority`
        // selects the bucket within. Drain order:
        //   for each bucket in priority order (Critical first):
        //     drain EDF heap (deadline-priority), then FIFO bitmap
        //     (registration-order)
        // Default workloads — every entry on the auto-default Fifo SC
        // (Normal priority) — populate only `fifo[Normal]`, so
        // dispatch order is bit-identical to 110.B.b for those.
        const NB: usize = super::sched_context::Priority::COUNT;
        let mut result = SpinOnceResult::new();
        let mut fifo: super::ready_set::BucketedFifoSet<NB, { crate::config::MAX_CBS }> =
            super::ready_set::BucketedFifoSet::new();
        let mut edf: super::ready_set::BucketedEdfSet<NB, { crate::config::MAX_CBS }> =
            super::ready_set::BucketedEdfSet::new();
        let active_mask = bits | always_mask;

        // Phase 110.E — refill any Sporadic SC budgets at period
        // boundaries before deciding what to dispatch this cycle.
        // Refill is polled (not ISR-driven) — coarse but correct
        // upper-bound bandwidth limiter.
        #[cfg(feature = "std")]
        {
            // Monotonic ms relative to a process-static epoch so the
            // refill clock survives wall-clock jumps.
            use std::sync::OnceLock;
            static EPOCH: OnceLock<std::time::Instant> = OnceLock::new();
            let now_ms = std::time::Instant::now()
                .saturating_duration_since(*EPOCH.get_or_init(std::time::Instant::now))
                .as_millis() as u64;
            // Use the cycle's `delta_ms` as the per-SC consumption
            // estimate — worst-case attribution. Per-callback
            // measurement lands with a higher-precision clock hook.
            let delta_us = (delta_ms as u32).saturating_mul(1000).min(u32::MAX);
            for slot in self.sporadic_states.iter_mut().flatten() {
                let _ = slot.tick(now_ms, delta_us);
            }
        }

        for i in 0..crate::config::MAX_CBS {
            if active_mask & (1u64 << i) == 0 {
                continue;
            }
            let sc_idx = self.sched_context_bindings[i].0 as usize;
            let sc_class_priority_deadline = self
                .sched_contexts
                .get(sc_idx)
                .and_then(|s| s.as_ref())
                .map(|sc| {
                    (
                        sc.class,
                        sc.priority.index(),
                        sc.deadline_us.get().map(|nz| nz.get()).unwrap_or(u32::MAX),
                    )
                });
            let (sc_class, bucket, deadline_us) = sc_class_priority_deadline.unwrap_or((
                super::sched_context::SchedClass::Fifo,
                super::sched_context::Priority::Normal.index(),
                u32::MAX,
            ));
            // Phase 110.E — Sporadic SC dispatch is suppressed when
            // its budget is exhausted. `tick` already refilled at
            // period boundary above; here we just gate.
            if matches!(sc_class, super::sched_context::SchedClass::Sporadic) {
                let has_budget = self
                    .sporadic_states
                    .get(sc_idx)
                    .and_then(|s| s.as_ref())
                    .map(|s| s.budget_remaining_us > 0)
                    .unwrap_or(true);
                if !has_budget {
                    continue;
                }
            }
            // Phase 110.G — TT window gate, orthogonal to class.
            // Skips dispatch when the SC has a TT window AND the
            // current monotonic time is outside it. Both gates apply
            // independently — a Sporadic SC with a TT window must
            // pass both.
            if self.major_frame_us > 0 {
                let sc_opt = self.sched_contexts.get(sc_idx).and_then(|s| s.as_ref());
                if let Some(sc) = sc_opt {
                    let off = sc.tt_window_offset_us.get().map(|nz| nz.get()).unwrap_or(0);
                    let dur = sc
                        .tt_window_duration_us
                        .get()
                        .map(|nz| nz.get())
                        .unwrap_or(0);
                    if dur > 0 {
                        // Compute current phase within the major
                        // frame using the accumulated `delta_ms` clock
                        // (std-only precise; no_std uses `delta_ms`
                        // approximation from spin cadence).
                        #[cfg(feature = "std")]
                        let now_us = {
                            use std::sync::OnceLock;
                            static EPOCH: OnceLock<std::time::Instant> = OnceLock::new();
                            std::time::Instant::now()
                                .saturating_duration_since(
                                    *EPOCH.get_or_init(std::time::Instant::now),
                                )
                                .as_micros() as u64
                        };
                        #[cfg(not(feature = "std"))]
                        let now_us = (delta_ms.saturating_mul(1000)) as u64;
                        let phase = (now_us % self.major_frame_us as u64) as u32;
                        let in_window = if off + dur <= self.major_frame_us {
                            phase >= off && phase < off + dur
                        } else {
                            // Window wraps the major frame boundary.
                            let end = (off as u64 + dur as u64) % self.major_frame_us as u64;
                            phase >= off || (phase as u64) < end
                        };
                        if !in_window {
                            continue;
                        }
                    }
                }
            }
            let is_edf = matches!(sc_class, super::sched_context::SchedClass::Edf);
            let job = super::types::ActiveJob {
                sort_key: if is_edf { deadline_us } else { i as u32 },
                desc_idx: i as super::types::DescIdx,
            };
            if is_edf {
                let _ = edf.insert_into(bucket, job);
            } else {
                let _ = fifo.insert_into(bucket, job);
            }
        }

        // SAFETY: each `desc_idx` we pop was set above only when the
        // corresponding `entries[i]` slot was `Some`; no Executor
        // mutation happens between that scan and this dispatch.
        let dispatch_one = |meta: &CallbackMeta,
                            arena_ptr: *mut u8,
                            delta_ms: u64,
                            result: &mut SpinOnceResult| {
            let data_ptr = unsafe { arena_ptr.add(meta.offset) };
            match unsafe { (meta.try_process)(data_ptr, delta_ms) } {
                Ok(true) => match meta.kind {
                    EntryKind::Subscription => result.subscriptions_processed += 1,
                    EntryKind::Service
                    | EntryKind::ServiceClient
                    | EntryKind::ActionServer
                    | EntryKind::ActionClient => result.services_handled += 1,
                    EntryKind::Timer => result.timers_fired += 1,
                    EntryKind::GuardCondition => {}
                },
                Ok(false) => {}
                Err(_) => match meta.kind {
                    EntryKind::Subscription => result.subscription_errors += 1,
                    EntryKind::Service
                    | EntryKind::ServiceClient
                    | EntryKind::ActionServer
                    | EntryKind::ActionClient => result.service_errors += 1,
                    EntryKind::Timer | EntryKind::GuardCondition => {}
                },
            }
        };

        // For each priority bucket (Critical → Normal → BestEffort),
        // drain EDF first then FIFO so an EDF callback in this bucket
        // beats a FIFO peer at the same priority, but no lower-priority
        // entry runs while a higher-priority bucket has work pending.
        // Strict static priority across buckets; non-preemptive within
        // an in-flight callback (see Phase 110.D).
        for bucket in 0..NB {
            while let Some(job) = edf.pop_from(bucket) {
                let i = job.desc_idx as usize;
                if let Some(meta) = self.entries[i].as_ref() {
                    dispatch_one(meta, arena_ptr, delta_ms, &mut result);
                }
            }
            while let Some(job) = fifo.pop_from(bucket) {
                let i = job.desc_idx as usize;
                if let Some(meta) = self.entries[i].as_ref() {
                    dispatch_one(meta, arena_ptr, delta_ms, &mut result);
                }
            }
        }

        // Process parameter services (outside the arena)
        #[cfg(feature = "param-services")]
        if let Some(params) = &mut self.params {
            let crate::parameter_services::ParamState { server, services } = &mut **params;
            if let Ok(n) = services.process_services(server) {
                result.services_handled += n;
            }
        }

        // Process lifecycle services (outside the arena).
        //
        // SAFETY: `change_state` dispatches a user-supplied C callback through a
        // raw function pointer stored in `LifecyclePollingNodeCtx`. The caller
        // of `register_lifecycle_services` guarantees the callback/context pair
        // stays live for as long as the executor (see that method's docs).
        #[cfg(feature = "lifecycle-services")]
        if let Some(lc) = &mut self.lifecycle {
            let crate::lifecycle_services::LifecycleRuntimeState {
                state_machine,
                services,
            } = &mut **lc;
            if let Ok(n) = unsafe { services.process_services(state_machine) } {
                result.services_handled += n;
            }
        }

        result
    }

    /// Drive I/O and dispatch callbacks in an infinite loop.
    ///
    /// Each iteration calls [`spin_once(timeout_ms)`](Self::spin_once),
    /// which pumps the transport and dispatches all registered callbacks.
    ///
    /// This is the primary run loop for embedded applications:
    ///
    /// ```ignore
    /// let mut executor = Executor::open(&config)?;
    /// executor.add_subscription::<Int32, _>("/topic", |msg| { /* ... */ })?;
    /// executor.spin(10); // never returns
    /// ```
    pub fn spin(&mut self, timeout: core::time::Duration) -> ! {
        loop {
            self.spin_once(timeout);
        }
    }

    /// Drive I/O and dispatch callbacks asynchronously.
    ///
    /// Runs forever, yielding between poll cycles so that other async tasks
    /// (e.g., [`Promise`](super::handles::Promise)) can make progress.
    ///
    /// Uses only `core::future` — no external async runtime dependency.
    ///
    /// # Usage patterns
    ///
    /// ```ignore
    /// // Pattern 1: select with a promise (embassy-futures)
    /// use embassy_futures::select::{select, Either};
    /// let promise = client.call(&req)?;
    /// let Either::Second(reply) = select(executor.spin_async(), promise).await
    ///     else { unreachable!() };
    ///
    /// // Pattern 2: manual polling (no async runtime)
    /// let mut promise = client.call(&req)?;
    /// loop {
    ///     executor.spin_once(core::time::Duration::from_millis(10));
    ///     if let Ok(Some(r)) = promise.try_recv() { break r; }
    /// }
    /// ```
    pub async fn spin_async(&mut self) -> ! {
        loop {
            self.spin_once(core::time::Duration::from_millis(1));
            core::future::poll_fn::<(), _>(|cx| {
                cx.waker().wake_by_ref();
                core::task::Poll::Pending
            })
            .await;
        }
    }

    // ========================================================================
    // spin_one_period (no_std)
    // ========================================================================

    /// Process one iteration and return remaining sleep time.
    ///
    /// This is `no_std` compatible — the caller is responsible for the actual
    /// delay using platform-specific sleep.
    ///
    /// # Arguments
    /// * `period_ms` - Target period in milliseconds
    /// * `elapsed_ms` - Time elapsed since last call (used for timer ticking)
    ///
    /// # Example
    ///
    /// ```ignore
    /// loop {
    ///     let r = executor.spin_one_period(10, elapsed_ms);
    ///     platform_sleep_ms(r.remaining_ms);
    /// }
    /// ```
    pub fn spin_one_period(&mut self, period_ms: u64, elapsed_ms: u64) -> SpinPeriodPollingResult {
        let result = self.spin_once(core::time::Duration::from_millis(elapsed_ms));
        SpinPeriodPollingResult {
            work: result,
            remaining_ms: period_ms.saturating_sub(elapsed_ms),
        }
    }
}

// ============================================================================
// Parameter services (cfg param-services)
// ============================================================================

#[cfg(feature = "param-services")]
impl Executor {
    /// Register the 6 ROS 2 parameter services for this node.
    ///
    /// Creates service servers for `get_parameters`, `set_parameters`,
    /// `set_parameters_atomically`, `list_parameters`, `describe_parameters`,
    /// and `get_parameter_types`.
    ///
    /// The service names follow the ROS 2 convention: `/{namespace}/{node_name}/{suffix}`.
    /// For the default namespace `/`, this becomes `/{node_name}/{suffix}` (e.g.
    /// `/sentinel/list_parameters`).
    ///
    /// Parameter services are stored outside the arena and don't consume
    /// callback slots.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = ExecutorConfig::from_env().node_name("talker");
    /// let mut executor = Executor::open(&config)?;
    /// executor.register_parameter_services()?;
    /// executor.declare_parameter("start_value", ParameterValue::Integer(0));
    /// ```
    pub fn register_parameter_services(&mut self) -> Result<(), NodeError> {
        use crate::parameter_services::{
            DescribeParameters, GetParameterTypes, GetParameters, ListParameters,
            PARAM_SERVICE_BUFFER_SIZE, ParameterServiceServers, SetParameters,
            SetParametersAtomically,
        };
        use nros_core::RosService;

        type PSrv<Svc> = super::handles::EmbeddedServiceServer<
            Svc,
            PARAM_SERVICE_BUFFER_SIZE,
            PARAM_SERVICE_BUFFER_SIZE,
        >;

        // Build the node FQN from namespace + node_name, following ROS 2 convention.
        // Default namespace "/" → "/{node_name}"; otherwise "/{namespace}/{node_name}".
        let mut node_fqn = heapless::String::<256>::new();
        let ns: &str = &self.namespace;
        let nn: &str = &self.node_name;
        if ns.is_empty() || ns == "/" {
            node_fqn.push_str("/").map_err(|_| NodeError::NameTooLong)?;
            node_fqn.push_str(nn).map_err(|_| NodeError::NameTooLong)?;
        } else {
            node_fqn.push_str("/").map_err(|_| NodeError::NameTooLong)?;
            node_fqn
                .push_str(ns.trim_matches('/'))
                .map_err(|_| NodeError::NameTooLong)?;
            node_fqn.push_str("/").map_err(|_| NodeError::NameTooLong)?;
            node_fqn.push_str(nn).map_err(|_| NodeError::NameTooLong)?;
        }

        /// Build a service name like `{node_fqn}/{suffix}` and create the server handle.
        fn create_param_srv<Svc: RosService>(
            session: &mut session::ConcreteSession,
            node_fqn: &str,
            namespace: &str,
            node_name: &str,
            suffix: &str,
        ) -> Result<session::RmwServiceServer, NodeError> {
            let mut name = heapless::String::<256>::new();
            name.push_str(node_fqn)
                .map_err(|_| NodeError::NameTooLong)?;
            name.push_str("/").map_err(|_| NodeError::NameTooLong)?;
            name.push_str(suffix).map_err(|_| NodeError::NameTooLong)?;
            let mut info = ServiceInfo::new(&name, Svc::SERVICE_NAME, Svc::SERVICE_HASH)
                .with_namespace(namespace);
            if !node_name.is_empty() {
                info = info.with_node_name(node_name);
            }
            session
                .create_service_server(&info)
                .map_err(|_| NodeError::Transport(TransportError::ServiceServerCreationFailed))
        }

        let get_handle = create_param_srv::<GetParameters>(
            &mut self.session,
            &node_fqn,
            ns,
            nn,
            "get_parameters",
        )?;
        let set_handle = create_param_srv::<SetParameters>(
            &mut self.session,
            &node_fqn,
            ns,
            nn,
            "set_parameters",
        )?;
        let set_atomic_handle = create_param_srv::<SetParametersAtomically>(
            &mut self.session,
            &node_fqn,
            ns,
            nn,
            "set_parameters_atomically",
        )?;
        let list_handle = create_param_srv::<ListParameters>(
            &mut self.session,
            &node_fqn,
            ns,
            nn,
            "list_parameters",
        )?;
        let desc_handle = create_param_srv::<DescribeParameters>(
            &mut self.session,
            &node_fqn,
            ns,
            nn,
            "describe_parameters",
        )?;
        let types_handle = create_param_srv::<GetParameterTypes>(
            &mut self.session,
            &node_fqn,
            ns,
            nn,
            "get_parameter_types",
        )?;

        let servers = ParameterServiceServers::new(
            PSrv::<GetParameters> {
                handle: get_handle,
                req_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            PSrv::<SetParameters> {
                handle: set_handle,
                req_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            PSrv::<SetParametersAtomically> {
                handle: set_atomic_handle,
                req_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            PSrv::<ListParameters> {
                handle: list_handle,
                req_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            PSrv::<DescribeParameters> {
                handle: desc_handle,
                req_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            PSrv::<GetParameterTypes> {
                handle: types_handle,
                req_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
        );

        self.params = Some(alloc::boxed::Box::new(
            crate::parameter_services::ParamState {
                server: nros_params::ParameterServer::new(),
                services: alloc::boxed::Box::new(servers),
            },
        ));

        Ok(())
    }
}

// ============================================================================
// Lifecycle services (cfg lifecycle-services)
// ============================================================================

#[cfg(feature = "lifecycle-services")]
impl Executor {
    /// Register the five REP-2002 lifecycle services on this executor.
    ///
    /// After this call, `ros2 lifecycle set|get|list|nodes` can drive the
    /// stored [`LifecyclePollingNodeCtx`](crate::lifecycle::LifecyclePollingNodeCtx)
    /// through the node's lifecycle. The state machine is created fresh
    /// (starting in `Unconfigured`); callers register their transition
    /// callbacks via [`Executor::lifecycle_state_machine_mut`].
    ///
    /// # Safety
    /// Registered callbacks on the state machine are C FFI function pointers.
    /// The caller must keep the callback code and any context it captures
    /// valid for as long as the executor processes services.
    pub fn register_lifecycle_services(&mut self) -> Result<(), NodeError> {
        use crate::{
            lifecycle::LifecyclePollingNodeCtx,
            lifecycle_services::{
                ChangeState, GetAvailableStates, GetAvailableTransitions, GetState,
                LIFECYCLE_SERVICE_BUFFER_SIZE, LifecycleRuntimeState, LifecycleServiceServers,
            },
        };
        use nros_core::RosService;

        type LcSrv<Svc> = super::handles::EmbeddedServiceServer<
            Svc,
            LIFECYCLE_SERVICE_BUFFER_SIZE,
            LIFECYCLE_SERVICE_BUFFER_SIZE,
        >;

        // Build the node FQN from namespace + node_name (same convention as
        // register_parameter_services).
        let mut node_fqn = heapless::String::<256>::new();
        let ns: &str = &self.namespace;
        let nn: &str = &self.node_name;
        if ns.is_empty() || ns == "/" {
            node_fqn.push_str("/").map_err(|_| NodeError::NameTooLong)?;
            node_fqn.push_str(nn).map_err(|_| NodeError::NameTooLong)?;
        } else {
            node_fqn.push_str("/").map_err(|_| NodeError::NameTooLong)?;
            node_fqn
                .push_str(ns.trim_matches('/'))
                .map_err(|_| NodeError::NameTooLong)?;
            node_fqn.push_str("/").map_err(|_| NodeError::NameTooLong)?;
            node_fqn.push_str(nn).map_err(|_| NodeError::NameTooLong)?;
        }

        fn create_lc_srv<Svc: RosService>(
            session: &mut session::ConcreteSession,
            node_fqn: &str,
            namespace: &str,
            node_name: &str,
            suffix: &str,
        ) -> Result<session::RmwServiceServer, NodeError> {
            let mut name = heapless::String::<256>::new();
            name.push_str(node_fqn)
                .map_err(|_| NodeError::NameTooLong)?;
            name.push_str("/").map_err(|_| NodeError::NameTooLong)?;
            name.push_str(suffix).map_err(|_| NodeError::NameTooLong)?;
            let mut info = ServiceInfo::new(&name, Svc::SERVICE_NAME, Svc::SERVICE_HASH)
                .with_namespace(namespace);
            if !node_name.is_empty() {
                info = info.with_node_name(node_name);
            }
            session
                .create_service_server(&info)
                .map_err(|_| NodeError::Transport(TransportError::ServiceServerCreationFailed))
        }

        let cs_handle =
            create_lc_srv::<ChangeState>(&mut self.session, &node_fqn, ns, nn, "change_state")?;
        let gs_handle =
            create_lc_srv::<GetState>(&mut self.session, &node_fqn, ns, nn, "get_state")?;
        let gas_handle = create_lc_srv::<GetAvailableStates>(
            &mut self.session,
            &node_fqn,
            ns,
            nn,
            "get_available_states",
        )?;
        let gat_handle = create_lc_srv::<GetAvailableTransitions>(
            &mut self.session,
            &node_fqn,
            ns,
            nn,
            "get_available_transitions",
        )?;
        let gtg_handle = create_lc_srv::<GetAvailableTransitions>(
            &mut self.session,
            &node_fqn,
            ns,
            nn,
            "get_transition_graph",
        )?;

        let servers = LifecycleServiceServers::new(
            LcSrv::<ChangeState> {
                handle: cs_handle,
                req_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            LcSrv::<GetState> {
                handle: gs_handle,
                req_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            LcSrv::<GetAvailableStates> {
                handle: gas_handle,
                req_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            LcSrv::<GetAvailableTransitions> {
                handle: gat_handle,
                req_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            LcSrv::<GetAvailableTransitions> {
                handle: gtg_handle,
                req_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
        );

        self.lifecycle = Some(alloc::boxed::Box::new(LifecycleRuntimeState {
            state_machine: LifecyclePollingNodeCtx::new(),
            services: alloc::boxed::Box::new(servers),
        }));

        Ok(())
    }

    /// Mutable access to the lifecycle state machine, if registered.
    ///
    /// Used to register transition callbacks before spinning and to read the
    /// current state from application code.
    pub fn lifecycle_state_machine_mut(
        &mut self,
    ) -> Option<&mut crate::lifecycle::LifecyclePollingNodeCtx> {
        self.lifecycle.as_mut().map(|lc| &mut lc.state_machine)
    }

    /// Immutable access to the lifecycle state machine, if registered.
    pub fn lifecycle_state_machine(&self) -> Option<&crate::lifecycle::LifecyclePollingNodeCtx> {
        self.lifecycle.as_ref().map(|lc| &lc.state_machine)
    }
}

// ============================================================================
// Parameter declaration API (cfg param-services)
// ============================================================================

#[cfg(feature = "param-services")]
impl Executor {
    /// Declare a parameter with a value. Returns `true` if successful.
    pub fn declare_parameter(&mut self, name: &str, value: nros_params::ParameterValue) -> bool {
        if let Some(params) = &mut self.params {
            params.server.declare(name, value)
        } else {
            false
        }
    }

    /// Declare a parameter with a value and descriptor. Returns `true` if successful.
    pub fn declare_parameter_with_descriptor(
        &mut self,
        name: &str,
        value: nros_params::ParameterValue,
        descriptor: nros_params::ParameterDescriptor,
    ) -> bool {
        if let Some(params) = &mut self.params {
            params
                .server
                .declare_with_descriptor(name, value, Some(descriptor))
        } else {
            false
        }
    }

    /// Get a parameter value by name.
    pub fn get_parameter(&self, name: &str) -> Option<&nros_params::ParameterValue> {
        self.params.as_ref()?.server.get(name)
    }

    /// Get an integer parameter value by name (convenience).
    pub fn get_parameter_integer(&self, name: &str) -> Option<i64> {
        self.params.as_ref()?.server.get_integer(name)
    }

    /// Get a reference to the parameter server (if registered).
    pub fn params(&self) -> Option<&nros_params::ParameterServer> {
        self.params.as_ref().map(|p| &p.server)
    }

    /// Get a mutable reference to the parameter server (if registered).
    pub fn params_mut(&mut self) -> Option<&mut nros_params::ParameterServer> {
        self.params.as_mut().map(|p| &mut p.server)
    }

    /// Create a typed parameter builder (rclrs-compatible API).
    ///
    /// Returns a [`ParameterBuilder`] for fluent parameter declaration with
    /// `.default()`, `.description()`, `.range()`, and terminal methods
    /// `.mandatory()`, `.optional()`, or `.read_only()`.
    ///
    /// Returns [`NodeError::NotInitialized`] if parameter services have
    /// not been registered yet — call [`register_parameter_services`]
    /// first.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let max_speed = executor.parameter::<f64>("max_speed")?
    ///     .default(25.0)
    ///     .description("Maximum velocity (m/s)")
    ///     .read_only()?;
    /// ```
    ///
    /// [`ParameterBuilder`]: nros_params::ParameterBuilder
    /// [`register_parameter_services`]: Self::register_parameter_services
    pub fn parameter<'a, T: nros_params::ParameterVariant>(
        &'a mut self,
        name: &'a str,
    ) -> Result<nros_params::ParameterBuilder<'a, T>, NodeError> {
        let server = self
            .params
            .as_mut()
            .map(|p| &mut p.server)
            .ok_or(NodeError::NotInitialized)?;
        Ok(nros_params::ParameterBuilder::new(server, name))
    }
}

// ============================================================================
// std-gated spin and halt methods
// ============================================================================

#[cfg(feature = "std")]
impl Executor {
    /// Blocking spin loop with configurable exit conditions.
    ///
    /// Runs until one of:
    /// - [`halt()`](Self::halt) is called (from another thread or signal handler)
    /// - Timeout expires (if set in options)
    /// - Max callbacks reached (if set in options)
    /// - `only_next` is true (single iteration)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Spin forever until halted
    /// executor.spin_blocking(SpinOptions::default())?;
    ///
    /// // Spin with 5-second timeout
    /// executor.spin_blocking(SpinOptions::new().timeout_ms(5000))?;
    ///
    /// // Single iteration
    /// executor.spin_blocking(SpinOptions::spin_once())?;
    /// ```
    pub fn spin_blocking(&mut self, opts: SpinOptions) -> Result<(), NodeError> {
        use std::time::{Duration, Instant};

        const POLL_INTERVAL: core::time::Duration = core::time::Duration::from_millis(10);

        let start = Instant::now();
        let timeout = opts.timeout_ms.map(Duration::from_millis);
        let mut total_callbacks = 0usize;

        self.halt_flag
            .store(false, std::sync::atomic::Ordering::SeqCst);

        loop {
            if self.halt_flag.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }

            if timeout.is_some_and(|t| start.elapsed() >= t) {
                break;
            }

            let result = self.spin_once(POLL_INTERVAL);
            total_callbacks += result.total();

            if opts.max_callbacks.is_some_and(|max| total_callbacks >= max) {
                break;
            }

            if opts.only_next {
                break;
            }
        }

        Ok(())
    }

    /// Execute one period with wall-clock overrun detection.
    ///
    /// Calls [`spin_once()`](Self::spin_once), measures wall-clock time, sleeps
    /// for the remainder if under budget.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let period = std::time::Duration::from_millis(10);
    /// let result = executor.spin_one_period_timed(period);
    /// if result.overrun {
    ///     log::warn!("Period overrun: {:?}", result.elapsed);
    /// }
    /// ```
    pub fn spin_one_period_timed(
        &mut self,
        period: std::time::Duration,
    ) -> super::types::SpinPeriodResult {
        let start = std::time::Instant::now();
        let result = self.spin_once(period);
        let elapsed = start.elapsed();
        let overrun = elapsed > period;
        if !overrun {
            std::thread::sleep(period - elapsed);
        }
        super::types::SpinPeriodResult {
            work: result,
            overrun,
            elapsed,
        }
    }

    /// Spin at a fixed rate with drift compensation. Blocks until halted.
    ///
    /// Uses wall-clock time to maintain the target rate. The next invocation
    /// time is accumulated (not reset to `now + period`) to prevent cumulative
    /// drift.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // 100Hz control loop — blocks until halt() is called
    /// executor.spin_period(std::time::Duration::from_millis(10))?;
    /// ```
    pub fn spin_period(&mut self, period: std::time::Duration) -> Result<(), NodeError> {
        self.halt_flag
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let mut next_invocation = std::time::Instant::now() + period;

        loop {
            if self.halt_flag.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }

            self.spin_once(period);

            let now = std::time::Instant::now();
            if now < next_invocation {
                std::thread::sleep(next_invocation - now);
            }
            // Accumulate to prevent drift (not = now + period)
            next_invocation += period;
        }
        Ok(())
    }

    /// Request the executor to stop spinning.
    ///
    /// Sets a flag that causes [`spin_blocking()`](Self::spin_blocking) or
    /// [`spin_period()`](Self::spin_period) to exit on the next iteration.
    /// Safe to call from another thread or signal handler.
    pub fn halt(&self) {
        self.halt_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Phase 110.D.b — move this Executor onto a fresh OS thread,
    /// apply a per-thread scheduling policy via the caller-supplied
    /// `apply_policy` function, and run the spin loop until
    /// [`ThreadHandle::halt`] fires.
    ///
    /// The function-pointer indirection on `apply_policy` lets the
    /// caller pass any platform's `PlatformScheduler::set_current_thread_policy`
    /// without forcing `Executor` to be generic over the platform —
    /// keeps the existing `Executor` type stable.
    ///
    /// Multi-executor preemption (the actual hard-RT win) comes from
    /// the OS scheduler — call `open_threaded` once per criticality
    /// tier, each with its own policy / priority. The kernel handles
    /// preemption across executors; within a single executor,
    /// dispatch remains non-preemptive (110.A–C bucketed sets).
    ///
    /// # Safety
    ///
    /// Moves `self` across thread boundaries. `Executor` contains a
    /// raw `*mut session::ConcreteSession` when constructed via
    /// `from_session_ptr`; the caller must ensure that pointer's
    /// referent stays valid across the lifetime of the spawned thread
    /// and that no other thread mutates the session concurrently.
    /// `from_session` (Owned) is safer — `ConcreteSession` ownership
    /// transfers cleanly into the thread.
    #[cfg(feature = "std")]
    pub unsafe fn open_threaded(
        self,
        policy: nros_platform_api::SchedPolicy,
        apply_policy: fn(
            nros_platform_api::SchedPolicy,
        ) -> Result<(), nros_platform_api::SchedError>,
        spin_period: core::time::Duration,
    ) -> ThreadHandle {
        let halt = std::sync::Arc::clone(&self.halt_flag);
        // SAFETY: Send is asserted via `unsafe impl Send for Executor`
        // below; the caller's safety contract on `from_session_ptr`
        // covers the pointer-validity invariant.
        let mut executor = self;
        let join = std::thread::spawn(move || {
            // Apply the requested OS scheduling policy to this fresh
            // thread. Failure is reported but not propagated — a
            // runtime that fails to lift to SCHED_FIFO still spins
            // correctly at SCHED_OTHER (just without RT guarantees).
            let _ = apply_policy(policy);
            while !executor.is_halted() {
                executor.spin_once(spin_period);
            }
        });
        ThreadHandle {
            join: Some(join),
            halt,
        }
    }

    /// Check if halt has been requested.
    pub fn is_halted(&self) -> bool {
        self.halt_flag.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Get a clone of the halt flag for use in signal handlers or other threads.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let halt = executor.halt_flag();
    /// std::thread::spawn(move || {
    ///     std::thread::sleep(Duration::from_secs(5));
    ///     halt.store(true, Ordering::SeqCst);
    /// });
    /// executor.spin_blocking(SpinOptions::default())?;
    /// ```
    pub fn halt_flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.halt_flag.clone()
    }
}

/// Phase 110.E.b — opaque per-platform timer handle. Stores the
/// raw platform handle (POSIX `timer_t` boxed via `PosixTimerHandle`,
/// FreeRTOS `TimerHandle_t`, etc.) plus a destroy thunk so the
/// Executor can clean up without being generic over the platform.
///
/// Caller of `register_sporadic_timer` builds this via
/// `OpaqueTimerHandle::new(handle, destroy_fn)` after their
/// `PlatformTimer::create_periodic` call returns.
#[cfg(feature = "alloc")]
pub struct OpaqueTimerHandle {
    handle: *mut core::ffi::c_void,
    destroy_fn: extern "C" fn(*mut core::ffi::c_void),
}

#[cfg(feature = "alloc")]
unsafe impl Send for OpaqueTimerHandle {}
#[cfg(feature = "alloc")]
unsafe impl Sync for OpaqueTimerHandle {}

#[cfg(feature = "alloc")]
impl OpaqueTimerHandle {
    /// # Safety
    /// `handle` must be a live platform-specific timer handle that
    /// `destroy_fn` knows how to drop. Caller surrenders ownership
    /// of the underlying handle to the Executor.
    pub unsafe fn new(
        handle: *mut core::ffi::c_void,
        destroy_fn: extern "C" fn(*mut core::ffi::c_void),
    ) -> Self {
        Self { handle, destroy_fn }
    }
}

#[cfg(feature = "alloc")]
impl Drop for OpaqueTimerHandle {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            (self.destroy_fn)(self.handle);
            self.handle = core::ptr::null_mut();
        }
    }
}

/// Handle returned from [`Executor::open_threaded`]. Holds the
/// spawned thread's join handle and a clone of the executor's halt
/// flag. Drop runs `halt() + join()` so the thread can't outlive the
/// handle.
#[cfg(feature = "std")]
pub struct ThreadHandle {
    join: Option<std::thread::JoinHandle<()>>,
    halt: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(feature = "std")]
impl ThreadHandle {
    /// Signal the spawned executor thread to stop. The thread exits
    /// on its next `spin_once` iteration.
    pub fn halt(&self) {
        self.halt.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Wait for the spawned thread to exit. Returns the join result.
    /// After `join`, calling it again is a no-op (returns `Ok(())`).
    pub fn join(mut self) -> std::thread::Result<()> {
        self.halt();
        match self.join.take() {
            Some(j) => j.join(),
            None => Ok(()),
        }
    }
}

#[cfg(feature = "std")]
impl Drop for ThreadHandle {
    fn drop(&mut self) {
        self.halt.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

// SAFETY: Phase 110.D.b — `Executor` contains a raw `*mut
// session::ConcreteSession` only on the `from_session_ptr` (Borrowed)
// path; the `from_session` (Owned) path is plain Send-able. The
// `unsafe fn open_threaded` entry point documents the safety
// contract for Borrowed sessions; for Owned sessions the Send claim
// is unconditional.
#[cfg(feature = "std")]
unsafe impl Send for Executor {}

impl Drop for Executor {
    fn drop(&mut self) {
        let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
        for meta in self.entries.iter().flatten() {
            // SAFETY: each entry was written by `ptr::write` in `add_*` and
            // has not been dropped yet. `drop_fn` matches the concrete type.
            unsafe {
                let data_ptr = arena_ptr.add(meta.offset);
                (meta.drop_fn)(data_ptr);
            }
        }
    }
}
