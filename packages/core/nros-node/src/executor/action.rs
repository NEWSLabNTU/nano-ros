//! Action server and client registration on the executor and handle types.

use core::marker::PhantomData;

use nros_core::RosAction;
use nros_rmw::{ActionInfo, QosSettings, ServiceInfo, Session, TopicInfo};

use super::{
    action_core::{ActionClientCore, ActionServerCore, RawActiveGoal},
    arena::{
        ActionClientRawArenaEntry, ActionServerArenaEntry, ActionServerRawArenaEntry, CallbackMeta,
        EntryKind, action_client_raw_try_process, action_server_raw_try_process,
        action_server_try_process, always_ready, as_active_goal_count, as_complete_goal,
        as_for_each_active_goal, as_publish_feedback, as_raw_active_goal_count,
        as_raw_complete_goal, as_raw_for_each_active_goal, as_raw_publish_feedback,
        as_raw_set_goal_status, as_set_goal_status, drop_entry, no_pre_sample,
    },
    handles::{ActionServer, ActiveGoal},
    spin::Executor,
    types::{
        HandleId, InvocationMode, NodeError, RawAcceptedCallback, RawCancelCallback,
        RawFeedbackCallback, RawGoalCallback, RawGoalResponseCallback, RawResultCallback,
    },
};

// ============================================================================
// Action server registration
// ============================================================================

impl Executor {
    /// Register an action server with goal/cancel callbacks.
    ///
    /// The executor automatically dispatches:
    /// - Goal acceptance via `goal_callback`
    /// - Cancel requests via `cancel_callback`
    /// - Result serving for completed goals
    ///
    /// Use the returned [`ActionServerHandle`] to publish feedback and complete goals.
    ///
    /// Uses default buffer sizes and max 4 concurrent goals.
    pub fn add_action_server<A, GoalF, CancelF>(
        &mut self,
        action_name: &str,
        goal_callback: GoalF,
        cancel_callback: CancelF,
    ) -> Result<ActionServerHandle<A>, NodeError>
    where
        A: RosAction + 'static,
        A::Goal: Clone,
        A::Result: Clone + Default,
        GoalF: FnMut(&nros_core::GoalId, &A::Goal) -> nros_core::GoalResponse + 'static,
        CancelF:
            FnMut(&nros_core::GoalId, nros_core::GoalStatus) -> nros_core::CancelResponse + 'static,
    {
        self.add_action_server_sized::<A, GoalF, CancelF, { crate::config::DEFAULT_RX_BUF_SIZE }, { crate::config::DEFAULT_RX_BUF_SIZE }, { crate::config::DEFAULT_RX_BUF_SIZE }, 4>(
            action_name,
            goal_callback,
            cancel_callback,
        )
    }

    /// Register an action server with custom buffer sizes.
    pub fn add_action_server_sized<
        A,
        GoalF,
        CancelF,
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
        const MAX_GOALS: usize,
    >(
        &mut self,
        action_name: &str,
        goal_callback: GoalF,
        cancel_callback: CancelF,
    ) -> Result<ActionServerHandle<A>, NodeError>
    where
        A: RosAction + 'static,
        A::Goal: Clone,
        A::Result: Clone + Default,
        GoalF: FnMut(&nros_core::GoalId, &A::Goal) -> nros_core::GoalResponse + 'static,
        CancelF:
            FnMut(&nros_core::GoalId, nros_core::GoalStatus) -> nros_core::CancelResponse + 'static,
    {
        type Entry<
            A,
            GoalF,
            CancelF,
            const GB: usize,
            const RB: usize,
            const FB: usize,
            const MG: usize,
        > = ActionServerArenaEntry<A, GoalF, CancelF, GB, RB, FB, MG>;

        let slot = self.next_entry_slot()?;

        // Create the action server entities (same logic as Node::create_action_server_sized)
        let action_info = ActionInfo::new(action_name, A::ACTION_NAME, A::ACTION_HASH);

        let send_goal_keyexpr: heapless::String<256> = action_info.send_goal_key();
        let send_goal_info =
            ServiceInfo::new(&send_goal_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let send_goal_server = self
            .session
            .create_service_server(&send_goal_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let cancel_goal_keyexpr: heapless::String<256> = action_info.cancel_goal_key();
        let cancel_goal_info = ServiceInfo::new(
            &cancel_goal_keyexpr,
            "action_msgs::srv::dds_::CancelGoal_",
            A::ACTION_HASH,
        )
        .with_domain(0);
        let cancel_goal_server = self
            .session
            .create_service_server(&cancel_goal_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let get_result_keyexpr: heapless::String<256> = action_info.get_result_key();
        let get_result_info =
            ServiceInfo::new(&get_result_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let get_result_server = self
            .session
            .create_service_server(&get_result_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let feedback_keyexpr: heapless::String<256> = action_info.feedback_key();
        let feedback_topic =
            TopicInfo::new(&feedback_keyexpr, A::ACTION_NAME, A::ACTION_HASH).with_domain(0);
        let feedback_publisher = self
            .session
            .create_publisher(&feedback_topic, QosSettings::BEST_EFFORT)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let status_keyexpr: heapless::String<256> = action_info.status_key();
        let status_topic = TopicInfo::new(
            &status_keyexpr,
            "action_msgs::msg::dds_::GoalStatusArray_",
            A::ACTION_HASH,
        )
        .with_domain(0);
        let status_publisher = self
            .session
            .create_publisher(&status_topic, QosSettings::BEST_EFFORT)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let server = ActionServer {
            core: super::action_core::ActionServerCore {
                send_goal_server,
                cancel_goal_server,
                get_result_server,
                feedback_publisher,
                status_publisher,
                active_goals: heapless::Vec::new(),
                completed_results: heapless::Vec::new(),
                result_slab: [0u8; RESULT_BUF],
                result_slab_used: 0,
                goal_buffer: [0u8; GOAL_BUF],
                feedback_buffer: [0u8; FEEDBACK_BUF],
                cancel_buffer: [0u8; 256],
            },
            typed_goals: heapless::Vec::new(),
            completed_goals: heapless::Vec::new(),
        };

        let offset = self
            .arena_alloc::<Entry<A, GoalF, CancelF, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>>(
            )?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset)
                as *mut Entry<A, GoalF, CancelF, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    server,
                    goal_callback,
                    cancel_callback,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::ActionServer,
            has_data: always_ready,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::Always,
            try_process: action_server_try_process::<
                A,
                GoalF,
                CancelF,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            drop_fn: drop_entry::<
                Entry<A, GoalF, CancelF, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>,
            >,
        });

        Ok(ActionServerHandle {
            entry_index: slot,
            publish_feedback_fn: as_publish_feedback::<
                A,
                GoalF,
                CancelF,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            complete_goal_fn: as_complete_goal::<
                A,
                GoalF,
                CancelF,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            set_goal_status_fn: as_set_goal_status::<
                A,
                GoalF,
                CancelF,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            active_goal_count_fn: as_active_goal_count::<
                A,
                GoalF,
                CancelF,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            for_each_active_goal_fn: as_for_each_active_goal::<
                A,
                GoalF,
                CancelF,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            _phantom: PhantomData,
        })
    }
}

// ============================================================================
// Handle types for arena-registered action server
// ============================================================================

/// Handle to an action server registered in the executor's arena.
///
/// Returned by [`Executor::add_action_server()`]. Provides methods
/// to interact with the server (publish feedback, complete goals) while the
/// executor automatically handles goal acceptance, cancel requests, and
/// result serving during [`spin_once()`](Executor::spin_once).
#[allow(clippy::type_complexity)]
pub struct ActionServerHandle<A: RosAction> {
    pub(crate) entry_index: usize,
    publish_feedback_fn:
        unsafe fn(*mut u8, &nros_core::GoalId, &A::Feedback) -> Result<(), NodeError>,
    complete_goal_fn: unsafe fn(*mut u8, &nros_core::GoalId, nros_core::GoalStatus, A::Result),
    set_goal_status_fn: unsafe fn(*mut u8, &nros_core::GoalId, nros_core::GoalStatus),
    active_goal_count_fn: unsafe fn(*const u8) -> usize,
    for_each_active_goal_fn: unsafe fn(*const u8, &mut dyn FnMut(&ActiveGoal<A>)),
    _phantom: PhantomData<A>,
}

impl<A: RosAction> Clone for ActionServerHandle<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: RosAction> Copy for ActionServerHandle<A> {}

impl<A: RosAction> ActionServerHandle<A> {
    /// Get the [`HandleId`] for this action server.
    ///
    /// Used with `Trigger::One` or `HandleSet` for trigger configuration.
    pub fn handle_id(&self) -> HandleId {
        HandleId(self.entry_index)
    }

    /// Publish feedback for an active goal.
    ///
    /// Serialises the feedback message and sends it to all clients
    /// monitoring this goal. Returns an error if the handle slot has
    /// been removed from the executor.
    pub fn publish_feedback(
        &self,
        executor: &mut Executor,
        goal_id: &nros_core::GoalId,
        feedback: &A::Feedback,
    ) -> Result<(), NodeError> {
        let meta = executor.entries[self.entry_index]
            .as_ref()
            .ok_or(NodeError::BufferTooSmall)?;
        let arena_ptr = executor.arena.as_mut_ptr() as *mut u8;
        unsafe {
            let data_ptr = arena_ptr.add(meta.offset);
            (self.publish_feedback_fn)(data_ptr, goal_id, feedback)
        }
    }

    /// Complete a goal with a terminal status and result payload.
    ///
    /// The goal is moved from the active set to the completed-results
    /// slab. Clients waiting on a result will receive the response.
    /// `status` should be one of `Succeeded`, `Aborted`, or `Canceled`.
    pub fn complete_goal(
        &self,
        executor: &mut Executor,
        goal_id: &nros_core::GoalId,
        status: nros_core::GoalStatus,
        result: A::Result,
    ) {
        if let Some(meta) = executor.entries[self.entry_index].as_ref() {
            let arena_ptr = executor.arena.as_mut_ptr() as *mut u8;
            unsafe {
                let data_ptr = arena_ptr.add(meta.offset);
                (self.complete_goal_fn)(data_ptr, goal_id, status, result);
            }
        }
    }

    /// Update a goal's status without completing it.
    ///
    /// Use this to transition a goal to `Executing` or `Canceling`
    /// while it is still active. To finish a goal, use [`complete_goal`](Self::complete_goal).
    pub fn set_goal_status(
        &self,
        executor: &mut Executor,
        goal_id: &nros_core::GoalId,
        status: nros_core::GoalStatus,
    ) {
        if let Some(meta) = executor.entries[self.entry_index].as_ref() {
            let arena_ptr = executor.arena.as_mut_ptr() as *mut u8;
            unsafe {
                let data_ptr = arena_ptr.add(meta.offset);
                (self.set_goal_status_fn)(data_ptr, goal_id, status);
            }
        }
    }

    /// Get the number of currently active goals.
    ///
    /// Returns 0 if the action server handle has been removed from the executor.
    pub fn active_goal_count(&self, executor: &Executor) -> usize {
        match executor.entries[self.entry_index].as_ref() {
            Some(meta) => {
                let arena_ptr = executor.arena.as_ptr() as *const u8;
                unsafe {
                    let data_ptr = arena_ptr.add(meta.offset);
                    (self.active_goal_count_fn)(data_ptr)
                }
            }
            None => 0,
        }
    }

    /// Iterate over all currently active goals.
    ///
    /// Calls `f` for each goal that has been accepted but not yet
    /// completed. Useful for monitoring progress or canceling stale goals.
    pub fn for_each_active_goal(&self, executor: &Executor, mut f: impl FnMut(&ActiveGoal<A>)) {
        if let Some(meta) = executor.entries[self.entry_index].as_ref() {
            let arena_ptr = executor.arena.as_ptr() as *const u8;
            unsafe {
                let data_ptr = arena_ptr.add(meta.offset);
                (self.for_each_active_goal_fn)(data_ptr, &mut f);
            }
        }
    }
}

// ============================================================================
// Raw (untyped) action server registration
// ============================================================================

impl Executor {
    /// Register a raw action server with raw-bytes callbacks.
    ///
    /// Unlike [`add_action_server()`](Executor::add_action_server), this does
    /// not require `RosAction` — the goal/cancel callbacks receive raw CDR
    /// bytes. This is used by the C API thin wrapper.
    ///
    /// `type_name` and `type_hash` identify the action type for key expression
    /// construction and liveliness tokens.
    #[allow(clippy::too_many_arguments)]
    pub fn add_action_server_raw(
        &mut self,
        action_name: &str,
        type_name: &str,
        type_hash: &str,
        goal_callback: RawGoalCallback,
        cancel_callback: RawCancelCallback,
        accepted_callback: Option<RawAcceptedCallback>,
        context: *mut core::ffi::c_void,
    ) -> Result<ActionServerRawHandle, NodeError> {
        self.add_action_server_raw_sized::<{ crate::config::DEFAULT_RX_BUF_SIZE }, { crate::config::DEFAULT_RX_BUF_SIZE }, { crate::config::DEFAULT_RX_BUF_SIZE }, 4>(
            action_name,
            type_name,
            type_hash,
            goal_callback,
            cancel_callback,
            accepted_callback,
            context,
        )
    }

    /// Register a raw action server with custom buffer sizes.
    #[allow(clippy::too_many_arguments)]
    pub fn add_action_server_raw_sized<
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
        const MAX_GOALS: usize,
    >(
        &mut self,
        action_name: &str,
        type_name: &str,
        type_hash: &str,
        goal_callback: RawGoalCallback,
        cancel_callback: RawCancelCallback,
        accepted_callback: Option<RawAcceptedCallback>,
        context: *mut core::ffi::c_void,
    ) -> Result<ActionServerRawHandle, NodeError> {
        type Entry<const GB: usize, const RB: usize, const FB: usize, const MG: usize> =
            ActionServerRawArenaEntry<GB, RB, FB, MG>;

        let slot = self.next_entry_slot()?;

        let action_info = ActionInfo::new(action_name, type_name, type_hash);
        let node_name: heapless::String<64> = self.node_name.clone();
        let ns: heapless::String<64> = self.namespace.clone();

        // Thread node identity through each underlying ServiceInfo /
        // TopicInfo so the Zenoh shim declares a liveliness token for
        // each entity. Without `with_node_name`,
        // `declare_entity_liveliness` short-circuits and
        // `wait_for_action_server` has nothing to find — same fix as
        // `Node::create_action_server_sized` (commit ea5e80b4).
        let send_goal_keyexpr: heapless::String<256> = action_info.send_goal_key();
        let mut send_goal_info =
            ServiceInfo::new(&send_goal_keyexpr, type_name, type_hash).with_namespace(&ns);
        if !node_name.is_empty() {
            send_goal_info = send_goal_info.with_node_name(&node_name);
        }
        let send_goal_server = self
            .session
            .create_service_server(&send_goal_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let cancel_goal_keyexpr: heapless::String<256> = action_info.cancel_goal_key();
        let mut cancel_goal_info = ServiceInfo::new(
            &cancel_goal_keyexpr,
            "action_msgs::srv::dds_::CancelGoal_",
            type_hash,
        )
        .with_namespace(&ns);
        if !node_name.is_empty() {
            cancel_goal_info = cancel_goal_info.with_node_name(&node_name);
        }
        let cancel_goal_server = self
            .session
            .create_service_server(&cancel_goal_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let get_result_keyexpr: heapless::String<256> = action_info.get_result_key();
        let mut get_result_info =
            ServiceInfo::new(&get_result_keyexpr, type_name, type_hash).with_namespace(&ns);
        if !node_name.is_empty() {
            get_result_info = get_result_info.with_node_name(&node_name);
        }
        let get_result_server = self
            .session
            .create_service_server(&get_result_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let feedback_keyexpr: heapless::String<256> = action_info.feedback_key();
        let mut feedback_topic =
            TopicInfo::new(&feedback_keyexpr, type_name, type_hash).with_namespace(&ns);
        if !node_name.is_empty() {
            feedback_topic = feedback_topic.with_node_name(&node_name);
        }
        let feedback_publisher = self
            .session
            .create_publisher(&feedback_topic, QosSettings::BEST_EFFORT)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let status_keyexpr: heapless::String<256> = action_info.status_key();
        let mut status_topic = TopicInfo::new(
            &status_keyexpr,
            "action_msgs::msg::dds_::GoalStatusArray_",
            type_hash,
        )
        .with_namespace(&ns);
        if !node_name.is_empty() {
            status_topic = status_topic.with_node_name(&node_name);
        }
        let status_publisher = self
            .session
            .create_publisher(&status_topic, QosSettings::BEST_EFFORT)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let core = ActionServerCore {
            send_goal_server,
            cancel_goal_server,
            get_result_server,
            feedback_publisher,
            status_publisher,
            active_goals: heapless::Vec::new(),
            completed_results: heapless::Vec::new(),
            result_slab: [0u8; RESULT_BUF],
            result_slab_used: 0,
            goal_buffer: [0u8; GOAL_BUF],
            feedback_buffer: [0u8; FEEDBACK_BUF],
            cancel_buffer: [0u8; 256],
        };

        let offset = self.arena_alloc::<Entry<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr =
                arena_ptr.add(offset) as *mut Entry<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    core,
                    goal_callback,
                    cancel_callback,
                    accepted_callback,
                    context,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::ActionServer,
            has_data: always_ready,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::Always,
            try_process: action_server_raw_try_process::<
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            drop_fn: drop_entry::<Entry<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>>,
        });

        Ok(ActionServerRawHandle {
            entry_index: slot,
            publish_feedback_fn: as_raw_publish_feedback::<
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            complete_goal_fn: as_raw_complete_goal::<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>,
            set_goal_status_fn: as_raw_set_goal_status::<
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            active_goal_count_fn: as_raw_active_goal_count::<
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            for_each_active_goal_fn: as_raw_for_each_active_goal::<
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
        })
    }
}

// ============================================================================
// Raw action server handle
// ============================================================================

/// Handle to a raw (untyped) action server registered in the executor's arena.
///
/// Returned by [`Executor::add_action_server_raw()`]. Provides methods
/// to interact with the server using raw CDR bytes.
#[repr(C)]
#[allow(clippy::type_complexity)]
pub struct ActionServerRawHandle {
    pub(crate) entry_index: usize,
    publish_feedback_fn:
        unsafe fn(*mut u8, &nros_core::GoalId, *const u8, usize) -> Result<(), NodeError>,
    complete_goal_fn:
        unsafe fn(*mut u8, &nros_core::GoalId, nros_core::GoalStatus, *const u8, usize),
    set_goal_status_fn: unsafe fn(*mut u8, &nros_core::GoalId, nros_core::GoalStatus),
    active_goal_count_fn: unsafe fn(*const u8) -> usize,
    for_each_active_goal_fn: unsafe fn(*const u8, &mut dyn FnMut(&RawActiveGoal)),
}

impl Clone for ActionServerRawHandle {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for ActionServerRawHandle {}

/// Sentinel value indicating an `ActionServerRawHandle` is not bound to an
/// arena entry yet. Used by Phase 87.5 to replace `Option<...>` with a
/// `#[repr(C)]`-compatible inline field.
///
/// Function pointers are populated with `unreachable_*` stubs that panic
/// if anyone is reckless enough to dispatch through an unbound handle —
/// callers must check `entry_index == INVALID_ENTRY_INDEX` first.
pub const INVALID_ENTRY_INDEX: usize = usize::MAX;

impl ActionServerRawHandle {
    /// Construct a sentinel handle representing "not registered yet".
    ///
    /// All function pointers are unreachable stubs; only valid use is
    /// to populate `#[repr(C)]` storage that is later overwritten by a
    /// real handle (or queried via `is_invalid()` to skip operations).
    pub const fn invalid() -> Self {
        unsafe fn unreachable_publish_feedback(
            _: *mut u8,
            _: &nros_core::GoalId,
            _: *const u8,
            _: usize,
        ) -> Result<(), NodeError> {
            unreachable!("ActionServerRawHandle::publish_feedback called on invalid handle")
        }
        unsafe fn unreachable_complete_goal(
            _: *mut u8,
            _: &nros_core::GoalId,
            _: nros_core::GoalStatus,
            _: *const u8,
            _: usize,
        ) {
            unreachable!("ActionServerRawHandle::complete_goal called on invalid handle")
        }
        unsafe fn unreachable_set_goal_status(
            _: *mut u8,
            _: &nros_core::GoalId,
            _: nros_core::GoalStatus,
        ) {
            unreachable!("ActionServerRawHandle::set_goal_status called on invalid handle")
        }
        unsafe fn unreachable_active_goal_count(_: *const u8) -> usize {
            unreachable!("ActionServerRawHandle::active_goal_count called on invalid handle")
        }
        unsafe fn unreachable_for_each_active_goal(
            _: *const u8,
            _: &mut dyn FnMut(&RawActiveGoal),
        ) {
            unreachable!("ActionServerRawHandle::for_each_active_goal called on invalid handle")
        }
        Self {
            entry_index: INVALID_ENTRY_INDEX,
            publish_feedback_fn: unreachable_publish_feedback,
            complete_goal_fn: unreachable_complete_goal,
            set_goal_status_fn: unreachable_set_goal_status,
            active_goal_count_fn: unreachable_active_goal_count,
            for_each_active_goal_fn: unreachable_for_each_active_goal,
        }
    }

    /// `true` if this handle is the sentinel returned by `Self::invalid()`.
    pub const fn is_invalid(&self) -> bool {
        self.entry_index == INVALID_ENTRY_INDEX
    }
}

impl Default for ActionServerRawHandle {
    fn default() -> Self {
        Self::invalid()
    }
}

impl ActionServerRawHandle {
    /// Get the [`HandleId`] for this action server.
    pub fn handle_id(&self) -> HandleId {
        HandleId(self.entry_index)
    }

    /// Publish feedback with raw CDR bytes (untyped variant).
    ///
    /// Used by the C API when feedback is already serialised.
    pub fn publish_feedback_raw(
        &self,
        executor: &mut Executor,
        goal_id: &nros_core::GoalId,
        feedback_data: &[u8],
    ) -> Result<(), NodeError> {
        let meta = executor.entries[self.entry_index]
            .as_ref()
            .ok_or(NodeError::BufferTooSmall)?;
        let arena_ptr = executor.arena.as_mut_ptr() as *mut u8;
        unsafe {
            let data_ptr = arena_ptr.add(meta.offset);
            (self.publish_feedback_fn)(
                data_ptr,
                goal_id,
                feedback_data.as_ptr(),
                feedback_data.len(),
            )
        }
    }

    /// Complete a goal with raw CDR result bytes (untyped variant).
    ///
    /// Moves the goal from the active set to the completed-results slab.
    pub fn complete_goal_raw(
        &self,
        executor: &mut Executor,
        goal_id: &nros_core::GoalId,
        status: nros_core::GoalStatus,
        result_data: &[u8],
    ) {
        if let Some(meta) = executor.entries[self.entry_index].as_ref() {
            let arena_ptr = executor.arena.as_mut_ptr() as *mut u8;
            unsafe {
                let data_ptr = arena_ptr.add(meta.offset);
                (self.complete_goal_fn)(
                    data_ptr,
                    goal_id,
                    status,
                    result_data.as_ptr(),
                    result_data.len(),
                );
            }
        }
    }

    /// Update a goal's status without completing it.
    ///
    /// Use this to transition a goal to `Executing` or `Canceling`
    /// while it is still active. To finish a goal, use [`complete_goal_raw`](Self::complete_goal_raw).
    pub fn set_goal_status(
        &self,
        executor: &mut Executor,
        goal_id: &nros_core::GoalId,
        status: nros_core::GoalStatus,
    ) {
        if let Some(meta) = executor.entries[self.entry_index].as_ref() {
            let arena_ptr = executor.arena.as_mut_ptr() as *mut u8;
            unsafe {
                let data_ptr = arena_ptr.add(meta.offset);
                (self.set_goal_status_fn)(data_ptr, goal_id, status);
            }
        }
    }

    /// Get the number of currently active goals.
    ///
    /// Returns 0 if the action server handle has been removed from the executor.
    pub fn active_goal_count(&self, executor: &Executor) -> usize {
        match executor.entries[self.entry_index].as_ref() {
            Some(meta) => {
                let arena_ptr = executor.arena.as_ptr() as *const u8;
                unsafe {
                    let data_ptr = arena_ptr.add(meta.offset);
                    (self.active_goal_count_fn)(data_ptr)
                }
            }
            None => 0,
        }
    }

    /// Iterate over all currently active goals (raw/untyped variant).
    ///
    /// Calls `f` for each goal that has been accepted but not yet completed.
    pub fn for_each_active_goal(&self, executor: &Executor, mut f: impl FnMut(&RawActiveGoal)) {
        if let Some(meta) = executor.entries[self.entry_index].as_ref() {
            let arena_ptr = executor.arena.as_ptr() as *const u8;
            unsafe {
                let data_ptr = arena_ptr.add(meta.offset);
                (self.for_each_active_goal_fn)(data_ptr, &mut f);
            }
        }
    }

    /// Look up the status of a single active goal by UUID.
    ///
    /// Returns `Some(status)` while the goal is still in the arena's
    /// `active_goals` vector. Returns `None` once the goal has been
    /// retired (completed + result delivered, or cancelled + acknowledged).
    ///
    /// This is the authoritative source of goal status — the C/C++ FFI
    /// layers call this from `nros_action_get_goal_status` rather than
    /// reading a cached field on their own handle structs.
    pub fn goal_status(
        &self,
        executor: &Executor,
        goal_id: &nros_core::GoalId,
    ) -> Option<nros_core::GoalStatus> {
        let mut found = None;
        self.for_each_active_goal(executor, |g| {
            if g.goal_id.uuid == goal_id.uuid && found.is_none() {
                found = Some(g.status);
            }
        });
        found
    }
}

// ============================================================================
// Action client registration
// ============================================================================

impl Executor {
    /// Register a raw action client with the executor.
    ///
    /// Creates service clients for send_goal, cancel_goal, get_result, and a
    /// feedback subscriber. The executor polls these during `spin_once` and
    /// invokes the provided callbacks when responses/feedback arrive.
    ///
    /// # Arguments
    /// * `action_name` — action name (e.g., "/fibonacci")
    /// * `type_name` — action type (e.g., "example_interfaces::action::dds_::Fibonacci_")
    /// * `type_hash` — type hash (e.g., "TypeHashNotSupported")
    /// * `goal_response_callback` — called when goal is accepted/rejected
    /// * `feedback_callback` — called when feedback is received
    /// * `result_callback` — called when result is received
    /// * `context` — opaque pointer passed to all callbacks
    #[allow(clippy::too_many_arguments)]
    pub fn add_action_client_raw(
        &mut self,
        action_name: &str,
        type_name: &str,
        type_hash: &str,
        goal_response_callback: Option<RawGoalResponseCallback>,
        feedback_callback: Option<RawFeedbackCallback>,
        result_callback: Option<RawResultCallback>,
        context: *mut core::ffi::c_void,
    ) -> Result<ActionClientRawHandle, NodeError> {
        self.add_action_client_raw_sized::<
            { crate::config::DEFAULT_RX_BUF_SIZE },
            { crate::config::DEFAULT_RX_BUF_SIZE },
            { crate::config::DEFAULT_RX_BUF_SIZE },
        >(
            action_name,
            type_name,
            type_hash,
            goal_response_callback,
            feedback_callback,
            result_callback,
            context,
        )
    }

    /// Register a raw action client with explicit buffer sizes.
    #[allow(clippy::too_many_arguments)]
    pub fn add_action_client_raw_sized<
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
    >(
        &mut self,
        action_name: &str,
        type_name: &str,
        type_hash: &str,
        goal_response_callback: Option<RawGoalResponseCallback>,
        feedback_callback: Option<RawFeedbackCallback>,
        result_callback: Option<RawResultCallback>,
        context: *mut core::ffi::c_void,
    ) -> Result<ActionClientRawHandle, NodeError> {
        type Entry<const GB: usize, const RB: usize, const FB: usize> =
            ActionClientRawArenaEntry<GB, RB, FB>;

        let slot = self.next_entry_slot()?;

        let action_info = ActionInfo::new(action_name, type_name, type_hash);
        let node_name: heapless::String<64> = self.node_name.clone();
        let ns: heapless::String<64> = self.namespace.clone();

        // Mirror `add_action_server_raw_sized`: thread node identity
        // through each underlying ServiceInfo / TopicInfo so the
        // client's per-entity liveliness tokens are declared and the
        // server-discovery wildcard built from `send_goal_info`
        // shares a domain with the matching server tokens.
        let send_goal_keyexpr: heapless::String<256> = action_info.send_goal_key();
        let mut send_goal_info =
            ServiceInfo::new(&send_goal_keyexpr, type_name, type_hash).with_namespace(&ns);
        if !node_name.is_empty() {
            send_goal_info = send_goal_info.with_node_name(&node_name);
        }
        let send_goal_client = self
            .session
            .create_service_client(&send_goal_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let cancel_goal_keyexpr: heapless::String<256> = action_info.cancel_goal_key();
        let mut cancel_goal_info = ServiceInfo::new(
            &cancel_goal_keyexpr,
            "action_msgs::srv::dds_::CancelGoal_",
            type_hash,
        )
        .with_namespace(&ns);
        if !node_name.is_empty() {
            cancel_goal_info = cancel_goal_info.with_node_name(&node_name);
        }
        let cancel_goal_client = self
            .session
            .create_service_client(&cancel_goal_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let get_result_keyexpr: heapless::String<256> = action_info.get_result_key();
        let mut get_result_info =
            ServiceInfo::new(&get_result_keyexpr, type_name, type_hash).with_namespace(&ns);
        if !node_name.is_empty() {
            get_result_info = get_result_info.with_node_name(&node_name);
        }
        let get_result_client = self
            .session
            .create_service_client(&get_result_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let feedback_keyexpr: heapless::String<256> = action_info.feedback_key();
        let mut feedback_topic =
            TopicInfo::new(&feedback_keyexpr, type_name, type_hash).with_namespace(&ns);
        if !node_name.is_empty() {
            feedback_topic = feedback_topic.with_node_name(&node_name);
        }
        let feedback_sub = self
            .session
            .create_subscriber(&feedback_topic, QosSettings::BEST_EFFORT)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let core = ActionClientCore::new(
            send_goal_client,
            cancel_goal_client,
            get_result_client,
            feedback_sub,
        );

        let offset = self.arena_alloc::<Entry<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut Entry<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    core,
                    goal_response_callback,
                    feedback_callback,
                    result_callback,
                    context,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::ActionClient,
            has_data: always_ready,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::Always,
            try_process: action_client_raw_try_process::<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>,
            drop_fn: drop_entry::<Entry<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>>,
        });

        Ok(ActionClientRawHandle { entry_index: slot })
    }
}

impl Executor {
    /// Register an existing `ActionClientCore` with the executor for async polling.
    ///
    /// Unlike `add_action_client_raw` (which creates new transport handles),
    /// this takes ownership of an existing core. Use this when the core was
    /// already created by the C/C++ action client init.
    pub fn add_action_client_core<
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
    >(
        &mut self,
        core: ActionClientCore<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>,
        goal_response_callback: Option<RawGoalResponseCallback>,
        feedback_callback: Option<RawFeedbackCallback>,
        result_callback: Option<RawResultCallback>,
        context: *mut core::ffi::c_void,
    ) -> Result<ActionClientRawHandle, NodeError> {
        type Entry<const GB: usize, const RB: usize, const FB: usize> =
            ActionClientRawArenaEntry<GB, RB, FB>;

        let slot = self.next_entry_slot()?;
        let offset = self.arena_alloc::<Entry<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut Entry<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    core,
                    goal_response_callback,
                    feedback_callback,
                    result_callback,
                    context,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::ActionClient,
            has_data: always_ready,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::Always,
            try_process: action_client_raw_try_process::<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>,
            drop_fn: drop_entry::<Entry<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>>,
        });

        Ok(ActionClientRawHandle { entry_index: slot })
    }
}

/// Handle returned by [`Executor::add_action_client_raw()`].
///
/// Provides methods to send goals, request results, and cancel goals
/// via the executor's non-blocking path.
pub struct ActionClientRawHandle {
    entry_index: usize,
}

impl ActionClientRawHandle {
    /// Get the entry index for this action client.
    pub fn entry_index(&self) -> usize {
        self.entry_index
    }
}
