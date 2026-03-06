//! Executor struct and core spin methods.

use core::marker::PhantomData;
use core::mem::MaybeUninit;

use nros_core::{RosMessage, RosService};
use nros_rmw::{
    QosSettings, ServiceInfo, ServiceServerTrait, Session, Subscriber, TopicInfo, TransportError,
};

use crate::timer::TimerDuration;

use super::arena::{
    CallbackMeta, EntryKind, GuardConditionEntry, SrvEntry, SrvRawEntry, SubEntry, SubInfoEntry,
    SubRawEntry, TimerEntry, TimerHeader, always_ready, drop_entry, guard_has_data,
    guard_try_process, no_pre_sample, srv_has_data, srv_raw_has_data, srv_raw_try_process,
    srv_try_process, sub_has_data, sub_info_has_data, sub_info_pre_sample, sub_info_try_process,
    sub_pre_sample, sub_raw_has_data, sub_raw_pre_sample, sub_raw_try_process, sub_try_process,
    timer_try_process,
};
#[cfg(feature = "safety-e2e")]
use super::arena::{
    SubSafetyEntry, sub_safety_has_data, sub_safety_pre_sample, sub_safety_try_process,
};
use super::node::Node;
#[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cffi"))]
use super::types::ExecutorConfig;
#[cfg(feature = "std")]
use super::types::SpinOptions;
use super::types::{
    ExecutorSemantics, GuardConditionHandle, HandleId, InvocationMode, NodeError,
    RawServiceCallback, RawSubscriptionCallback, ReadinessSnapshot, SpinOnceResult,
    SpinPeriodPollingResult, Trigger,
};

// ============================================================================
// Executor::open() factory methods
// ============================================================================

#[cfg(any(feature = "rmw-xrce", feature = "rmw-cffi"))]
use nros_rmw::Rmw;

#[cfg(feature = "rmw-zenoh")]
impl<const MAX_CBS: usize, const CB_ARENA: usize>
    Executor<nros_rmw_zenoh::ZenohSession, MAX_CBS, CB_ARENA>
{
    /// Open a new executor session using the zenoh-pico backend.
    ///
    /// Connects to the zenoh router at the locator specified in `config`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = ExecutorConfig::from_env().node_name("my_node");
    /// let mut executor = Executor::open(&config)?;
    /// ```
    pub fn open(config: &ExecutorConfig<'_>) -> Result<Self, NodeError> {
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
}

#[cfg(feature = "rmw-xrce")]
impl<const MAX_CBS: usize, const CB_ARENA: usize>
    Executor<nros_rmw_xrce::XrceSession, MAX_CBS, CB_ARENA>
{
    /// Open a new executor session using the XRCE-DDS backend.
    ///
    /// Automatically initializes the active POSIX transport (`posix-udp` or
    /// `posix-serial`) before connecting to the XRCE agent.
    pub fn open(config: &ExecutorConfig<'_>) -> Result<Self, NodeError> {
        // Auto-init transport based on active feature
        #[cfg(feature = "posix-udp")]
        unsafe {
            nros_rmw_xrce::posix_udp::init_posix_udp_transport(config.locator);
        }
        #[cfg(feature = "posix-serial")]
        unsafe {
            nros_rmw_xrce::posix_serial::init_posix_serial_transport(config.locator);
        }
        #[cfg(feature = "platform-zephyr")]
        unsafe {
            nros_rmw_xrce::zephyr::init_zephyr_transport(config.locator);
        }

        let rmw_config = nros_rmw::RmwConfig {
            locator: config.locator,
            mode: config.mode,
            domain_id: config.domain_id,
            node_name: config.node_name,
            namespace: config.namespace,
        };
        let session = nros_rmw_xrce::XrceRmw::open(&rmw_config)
            .map_err(|_| NodeError::Transport(TransportError::ConnectionFailed))?;
        let mut executor = Self::from_session(session);
        executor.set_node_identity(config.node_name, config.namespace);
        Ok(executor)
    }
}

#[cfg(feature = "rmw-cffi")]
impl<const MAX_CBS: usize, const CB_ARENA: usize>
    Executor<nros_rmw_cffi::CffiSession, MAX_CBS, CB_ARENA>
{
    /// Open a new executor session using the C FFI backend.
    pub fn open(config: &ExecutorConfig<'_>) -> Result<Self, NodeError> {
        let rmw_config = nros_rmw::RmwConfig {
            locator: config.locator,
            mode: config.mode,
            domain_id: config.domain_id,
            node_name: config.node_name,
            namespace: config.namespace,
        };
        let session = nros_rmw_cffi::CffiRmw::open(&rmw_config)
            .map_err(|_| NodeError::Transport(TransportError::ConnectionFailed))?;
        let mut executor = Self::from_session(session);
        executor.set_node_identity(config.node_name, config.namespace);
        Ok(executor)
    }
}

// ============================================================================
// SessionStore — owned or borrowed session
// ============================================================================

/// Session storage: owned or borrowed via raw pointer.
///
/// The C API creates a session in `nros_support_init()` before the
/// executor. `Borrowed` lets the executor use that session without owning it.
pub(crate) enum SessionStore<S> {
    Owned(S),
    Borrowed(*mut S),
}

impl<S> core::ops::Deref for SessionStore<S> {
    type Target = S;
    fn deref(&self) -> &S {
        match self {
            SessionStore::Owned(s) => s,
            SessionStore::Borrowed(ptr) => unsafe { &**ptr },
        }
    }
}

impl<S> core::ops::DerefMut for SessionStore<S> {
    fn deref_mut(&mut self) -> &mut S {
        match self {
            SessionStore::Owned(s) => s,
            SessionStore::Borrowed(ptr) => unsafe { &mut **ptr },
        }
    }
}

// ============================================================================
// Executor<S>
// ============================================================================

/// Backend-agnostic executor that owns a [`Session`].
///
/// Provides `create_node()` for entity creation and `drive_io()` for polling.
///
/// # Callback Mode
///
/// When `MAX_CBS > 0` and `CB_ARENA > 0`, the executor supports arena-based
/// callback registration via [`add_subscription()`](Self::add_subscription)
/// and [`add_service()`](Self::add_service), with dispatch via
/// [`spin_once()`](Self::spin_once). No heap allocation is needed.
///
/// The defaults are set via `NROS_EXECUTOR_MAX_CBS` (default 4) and
/// `NROS_EXECUTOR_ARENA_SIZE` (default 4096) environment variables at build time.
/// Set both to `0` for zero overhead in manual-polling code.
pub struct Executor<
    S,
    const MAX_CBS: usize = { crate::config::DEFAULT_MAX_CBS },
    const CB_ARENA: usize = { crate::config::DEFAULT_ARENA_SIZE },
> {
    pub(crate) session: SessionStore<S>,
    pub(crate) arena: [MaybeUninit<u8>; CB_ARENA],
    pub(crate) arena_used: usize,
    pub(crate) entries: [Option<CallbackMeta>; MAX_CBS],
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
}

impl<S: Session, const MAX_CBS: usize, const CB_ARENA: usize> Executor<S, MAX_CBS, CB_ARENA> {
    /// Create an executor from an already-opened session.
    pub fn from_session(session: S) -> Self {
        // SAFETY: MaybeUninit::uninit() is always safe; these bytes are only
        // accessed through properly-typed ptr::write / ptr::read via the
        // dispatch function pointers stored in `entries`.
        Self {
            session: SessionStore::Owned(session),
            arena: [MaybeUninit::uninit(); CB_ARENA],
            arena_used: 0,
            entries: [None; MAX_CBS],
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
        }
    }

    /// Create an executor from a borrowed session pointer.
    ///
    /// # Safety
    /// - `session_ptr` must point to a valid, initialized `S` that lives at
    ///   least as long as this executor.
    /// - The caller must not move or drop the session while the executor exists.
    pub unsafe fn from_session_ptr(session_ptr: *mut S) -> Self {
        Self {
            session: SessionStore::Borrowed(session_ptr),
            arena: [MaybeUninit::uninit(); CB_ARENA],
            arena_used: 0,
            entries: [None; MAX_CBS],
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

    /// Create a node on this executor.
    pub fn create_node(&mut self, name: &str) -> Result<Node<'_, S>, NodeError> {
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
            &mut *self.session,
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
    pub fn session(&self) -> &S {
        &self.session
    }

    /// Get a mutable reference to the underlying session.
    pub fn session_mut(&mut self) -> &mut S {
        &mut self.session
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
        if new_used > CB_ARENA {
            return Err(NodeError::BufferTooSmall);
        }
        self.arena_used = new_used;
        Ok(aligned_offset)
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
    ///     executor.spin_once(10);
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
        S::SubscriberHandle: Subscriber,
    {
        self.add_subscription_sized::<M, F, { crate::config::DEFAULT_RX_BUF_SIZE }>(
            topic_name, callback,
        )
    }

    /// Register a subscription callback with a custom receive buffer size.
    pub fn add_subscription_sized<M, F, const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        M: RosMessage + 'static,
        F: FnMut(&M) + 'static,
        S::SubscriberHandle: Subscriber,
    {
        type Entry<M, Sub, F, const N: usize> = SubEntry<M, Sub, F, N>;

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

        let offset = self.arena_alloc::<Entry<M, S::SubscriberHandle, F, RX_BUF>>()?;

        // SAFETY: `arena_alloc` guarantees the offset is within bounds and
        // properly aligned for `Entry`. We write a fully-initialized value.
        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut Entry<M, S::SubscriberHandle, F, RX_BUF>;
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
            try_process: sub_try_process::<M, S::SubscriberHandle, F, RX_BUF>,
            has_data: sub_has_data::<M, S::SubscriberHandle, F, RX_BUF>,
            pre_sample: sub_pre_sample::<M, S::SubscriberHandle, F, RX_BUF>,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<Entry<M, S::SubscriberHandle, F, RX_BUF>>,
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
        S::SubscriberHandle: Subscriber,
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
        S::SubscriberHandle: Subscriber,
    {
        type Entry<M, Sub, F, const N: usize> = SubInfoEntry<M, Sub, F, N>;

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

        let offset = self.arena_alloc::<Entry<M, S::SubscriberHandle, F, RX_BUF>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut Entry<M, S::SubscriberHandle, F, RX_BUF>;
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
            try_process: sub_info_try_process::<M, S::SubscriberHandle, F, RX_BUF>,
            has_data: sub_info_has_data::<M, S::SubscriberHandle, F, RX_BUF>,
            pre_sample: sub_info_pre_sample::<M, S::SubscriberHandle, F, RX_BUF>,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<Entry<M, S::SubscriberHandle, F, RX_BUF>>,
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
        S::SubscriberHandle: Subscriber,
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
        S::SubscriberHandle: Subscriber,
    {
        type Entry<M, Sub, F, const N: usize> = SubSafetyEntry<M, Sub, F, N>;

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

        let offset = self.arena_alloc::<Entry<M, S::SubscriberHandle, F, RX_BUF>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut Entry<M, S::SubscriberHandle, F, RX_BUF>;
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
            try_process: sub_safety_try_process::<M, S::SubscriberHandle, F, RX_BUF>,
            has_data: sub_safety_has_data::<M, S::SubscriberHandle, F, RX_BUF>,
            pre_sample: sub_safety_pre_sample::<M, S::SubscriberHandle, F, RX_BUF>,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<Entry<M, S::SubscriberHandle, F, RX_BUF>>,
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
        S::ServiceServerHandle: ServiceServerTrait,
        <S::ServiceServerHandle as ServiceServerTrait>::Error: From<TransportError>,
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
        S::ServiceServerHandle: ServiceServerTrait,
        <S::ServiceServerHandle as ServiceServerTrait>::Error: From<TransportError>,
    {
        type Entry<Svc, Srv, F, const RQ: usize, const RP: usize> = SrvEntry<Svc, Srv, F, RQ, RP>;

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

        let offset =
            self.arena_alloc::<Entry<Svc, S::ServiceServerHandle, F, REQ_BUF, REPLY_BUF>>()?;

        // SAFETY: same guarantees as add_subscription_sized.
        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset)
                as *mut Entry<Svc, S::ServiceServerHandle, F, REQ_BUF, REPLY_BUF>;
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
            try_process: srv_try_process::<Svc, S::ServiceServerHandle, F, REQ_BUF, REPLY_BUF>,
            has_data: srv_has_data::<Svc, S::ServiceServerHandle, F, REQ_BUF, REPLY_BUF>,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<Entry<Svc, S::ServiceServerHandle, F, REQ_BUF, REPLY_BUF>>,
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
    ) -> Result<HandleId, NodeError>
    where
        S::SubscriberHandle: Subscriber,
    {
        self.add_subscription_raw_with_qos_sized::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
            topic_name,
            type_name,
            type_hash,
            QosSettings::default(),
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
    ) -> Result<HandleId, NodeError>
    where
        S::SubscriberHandle: Subscriber,
    {
        self.add_subscription_raw_with_qos_sized::<RX_BUF>(
            topic_name,
            type_name,
            type_hash,
            QosSettings::default(),
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
    ) -> Result<HandleId, NodeError>
    where
        S::SubscriberHandle: Subscriber,
    {
        self.add_subscription_raw_with_qos_sized::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
            topic_name, type_name, type_hash, qos, callback, context,
        )
    }

    /// Register a raw subscription callback with custom QoS and buffer size.
    pub fn add_subscription_raw_with_qos_sized<const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QosSettings,
        callback: RawSubscriptionCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError>
    where
        S::SubscriberHandle: Subscriber,
    {
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

        let offset = self.arena_alloc::<SubRawEntry<S::SubscriberHandle, RX_BUF>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut SubRawEntry<S::SubscriberHandle, RX_BUF>;
            core::ptr::write(
                entry_ptr,
                SubRawEntry {
                    handle,
                    buffer: [0u8; RX_BUF],
                    sampled_len: 0,
                    callback,
                    context,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Subscription,
            try_process: sub_raw_try_process::<S::SubscriberHandle, RX_BUF>,
            has_data: sub_raw_has_data::<S::SubscriberHandle, RX_BUF>,
            pre_sample: sub_raw_pre_sample::<S::SubscriberHandle, RX_BUF>,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<SubRawEntry<S::SubscriberHandle, RX_BUF>>,
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
    ) -> Result<HandleId, NodeError>
    where
        S::ServiceServerHandle: ServiceServerTrait,
        <S::ServiceServerHandle as ServiceServerTrait>::Error: From<TransportError>,
    {
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
    ) -> Result<HandleId, NodeError>
    where
        S::ServiceServerHandle: ServiceServerTrait,
        <S::ServiceServerHandle as ServiceServerTrait>::Error: From<TransportError>,
    {
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

        let offset =
            self.arena_alloc::<SrvRawEntry<S::ServiceServerHandle, REQ_BUF, REPLY_BUF>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset)
                as *mut SrvRawEntry<S::ServiceServerHandle, REQ_BUF, REPLY_BUF>;
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
            try_process: srv_raw_try_process::<S::ServiceServerHandle, REQ_BUF, REPLY_BUF>,
            has_data: srv_raw_has_data::<S::ServiceServerHandle, REQ_BUF, REPLY_BUF>,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<SrvRawEntry<S::ServiceServerHandle, REQ_BUF, REPLY_BUF>>,
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
    // spin_once (three-phase: readiness → trigger → dispatch)
    // ========================================================================

    /// Drive I/O and dispatch registered callbacks once.
    ///
    /// Three-phase execution:
    /// 1. **Readiness scan** — query each handle's `has_data()`.
    /// 2. **Trigger evaluation** — check if the executor-level trigger passes.
    /// 3. **Dispatch** — invoke callbacks according to their `InvocationMode`.
    ///
    /// Returns a [`SpinOnceResult`] with counts of processed items and errors.
    pub fn spin_once(&mut self, timeout_ms: i32) -> SpinOnceResult {
        let _ = self.session.drive_io(timeout_ms);

        let delta_ms = timeout_ms.max(0) as u64;
        let arena_ptr = self.arena.as_mut_ptr() as *mut u8;

        // Phase 1: Readiness scan
        let mut bits: u64 = 0;
        let mut count: usize = 0;
        let mut non_timer_mask: u64 = 0;

        for (i, meta) in self.entries.iter().enumerate() {
            if let Some(meta) = meta {
                let data_ptr = unsafe { arena_ptr.add(meta.offset) as *const u8 };
                if unsafe { (meta.has_data)(data_ptr) } {
                    bits |= 1u64 << i;
                }
                if !matches!(meta.kind, EntryKind::Timer | EntryKind::GuardCondition) {
                    non_timer_mask |= 1u64 << i;
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

        // Phase 3: Dispatch
        let mut result = SpinOnceResult::new();

        for (i, meta) in self.entries.iter().enumerate() {
            let Some(meta) = meta else { continue };

            // Check invocation mode
            let should_fire = match meta.invocation {
                InvocationMode::OnNewData => bits & (1u64 << i) != 0,
                InvocationMode::Always => true,
            };

            if !should_fire {
                continue;
            }

            let data_ptr = unsafe { arena_ptr.add(meta.offset) };
            match unsafe { (meta.try_process)(data_ptr, delta_ms) } {
                Ok(true) => match meta.kind {
                    EntryKind::Subscription => {
                        result.subscriptions_processed += 1;
                    }
                    EntryKind::Service | EntryKind::ActionServer => {
                        result.services_handled += 1;
                    }
                    EntryKind::Timer => result.timers_fired += 1,
                    EntryKind::GuardCondition => {}
                },
                Ok(false) => {}
                Err(_) => match meta.kind {
                    EntryKind::Subscription => {
                        result.subscription_errors += 1;
                    }
                    EntryKind::Service | EntryKind::ActionServer => {
                        result.service_errors += 1;
                    }
                    EntryKind::Timer | EntryKind::GuardCondition => {}
                },
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
    /// let mut executor: Executor<_> = Executor::open(&config)?;
    /// executor.add_subscription::<Int32, _>("/topic", |msg| { /* ... */ })?;
    /// executor.spin(10); // never returns
    /// ```
    pub fn spin(&mut self, timeout_ms: i32) -> ! {
        loop {
            self.spin_once(timeout_ms);
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
    ///     executor.spin_once(10);
    ///     if let Ok(Some(r)) = promise.try_recv() { break r; }
    /// }
    /// ```
    pub async fn spin_async(&mut self) -> ! {
        loop {
            self.spin_once(1);
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
        let result = self.spin_once(elapsed_ms as i32);
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
impl<S: Session, const MAX_CBS: usize, const CB_ARENA: usize> Executor<S, MAX_CBS, CB_ARENA>
where
    S::ServiceServerHandle: ServiceServerTrait + 'static,
    <S::ServiceServerHandle as ServiceServerTrait>::Error: From<TransportError>,
{
    /// Register the 6 ROS 2 parameter services for this node.
    ///
    /// Creates service servers for `get_parameters`, `set_parameters`,
    /// `set_parameters_atomically`, `list_parameters`, `describe_parameters`,
    /// and `get_parameter_types` under the given node fully-qualified name.
    ///
    /// Parameter services are stored outside the arena and don't consume
    /// `MAX_CBS` slots.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut executor = Executor::open(&config)?;
    /// executor.register_parameter_services("/demo/talker")?;
    /// executor.declare_parameter("start_value", ParameterValue::Integer(0));
    /// ```
    pub fn register_parameter_services(&mut self, node_fqn: &str) -> Result<(), NodeError> {
        use crate::parameter_services::{
            DescribeParameters, GetParameterTypes, GetParameters, ListParameters,
            PARAM_SERVICE_BUFFER_SIZE, ParameterServiceServers, SetParameters,
            SetParametersAtomically,
        };
        use nros_core::RosService;

        type PSrv<Svc, Srv> = super::handles::EmbeddedServiceServer<
            Svc,
            Srv,
            PARAM_SERVICE_BUFFER_SIZE,
            PARAM_SERVICE_BUFFER_SIZE,
        >;

        /// Build a service name like `{node_fqn}/{suffix}` and create the server handle.
        fn create_param_srv<Svc: RosService, S: Session>(
            session: &mut S,
            node_fqn: &str,
            suffix: &str,
        ) -> Result<S::ServiceServerHandle, NodeError>
        where
            S::ServiceServerHandle: ServiceServerTrait,
            <S::ServiceServerHandle as ServiceServerTrait>::Error: From<TransportError>,
        {
            let mut name = heapless::String::<256>::new();
            name.push_str(node_fqn)
                .map_err(|_| NodeError::NameTooLong)?;
            name.push_str("/").map_err(|_| NodeError::NameTooLong)?;
            name.push_str(suffix).map_err(|_| NodeError::NameTooLong)?;
            let info = ServiceInfo::new(&name, Svc::SERVICE_NAME, Svc::SERVICE_HASH);
            session
                .create_service_server(&info)
                .map_err(|_| NodeError::Transport(TransportError::ServiceServerCreationFailed))
        }

        let get_handle =
            create_param_srv::<GetParameters, S>(&mut self.session, node_fqn, "get_parameters")?;
        let set_handle =
            create_param_srv::<SetParameters, S>(&mut self.session, node_fqn, "set_parameters")?;
        let set_atomic_handle = create_param_srv::<SetParametersAtomically, S>(
            &mut self.session,
            node_fqn,
            "set_parameters_atomically",
        )?;
        let list_handle =
            create_param_srv::<ListParameters, S>(&mut self.session, node_fqn, "list_parameters")?;
        let desc_handle = create_param_srv::<DescribeParameters, S>(
            &mut self.session,
            node_fqn,
            "describe_parameters",
        )?;
        let types_handle = create_param_srv::<GetParameterTypes, S>(
            &mut self.session,
            node_fqn,
            "get_parameter_types",
        )?;

        let servers = ParameterServiceServers::new(
            PSrv::<GetParameters, _> {
                handle: get_handle,
                req_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            PSrv::<SetParameters, _> {
                handle: set_handle,
                req_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            PSrv::<SetParametersAtomically, _> {
                handle: set_atomic_handle,
                req_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            PSrv::<ListParameters, _> {
                handle: list_handle,
                req_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            PSrv::<DescribeParameters, _> {
                handle: desc_handle,
                req_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            PSrv::<GetParameterTypes, _> {
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
    /// # Panics
    ///
    /// Panics if parameter services have not been registered
    /// (call [`register_parameter_services`] first).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let max_speed = executor.parameter::<f64>("max_speed")
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
    ) -> nros_params::ParameterBuilder<'a, T> {
        let server =
            self.params.as_mut().map(|p| &mut p.server).expect(
                "parameter services not registered — call register_parameter_services() first",
            );
        nros_params::ParameterBuilder::new(server, name)
    }
}

// ============================================================================
// std-gated spin and halt methods
// ============================================================================

#[cfg(feature = "std")]
impl<S: Session, const MAX_CBS: usize, const CB_ARENA: usize> Executor<S, MAX_CBS, CB_ARENA> {
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

        const POLL_INTERVAL_MS: i32 = 10;

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

            let result = self.spin_once(POLL_INTERVAL_MS);
            total_callbacks += result.total();

            if opts.max_callbacks.is_some_and(|max| total_callbacks >= max) {
                break;
            }

            if opts.only_next {
                break;
            }

            std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS as u64));
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
        let period_ms = period.as_millis() as i32;
        let result = self.spin_once(period_ms.max(1));
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

            let period_ms = period.as_millis() as i32;
            self.spin_once(period_ms.max(1));

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

impl<S, const MAX_CBS: usize, const CB_ARENA: usize> Drop for Executor<S, MAX_CBS, CB_ARENA> {
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
