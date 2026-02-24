//! Action server registration on the executor and handle types.

use core::marker::PhantomData;

use nros_core::RosAction;
use nros_rmw::{
    ActionInfo, Publisher, QosSettings, ServiceInfo, ServiceServerTrait, Session, TopicInfo,
};

use super::action_core::{ActionServerCore, RawActiveGoal};
use super::arena::{
    ActionServerArenaEntry, ActionServerRawArenaEntry, CallbackMeta, EntryKind,
    action_server_raw_try_process, action_server_try_process, always_ready, as_active_goal_count,
    as_complete_goal, as_for_each_active_goal, as_publish_feedback, as_raw_active_goal_count,
    as_raw_complete_goal, as_raw_for_each_active_goal, as_raw_publish_feedback,
    as_raw_set_goal_status, as_set_goal_status, drop_entry, no_pre_sample,
};
use super::handles::{ActionServer, ActiveGoal};
use super::spin::Executor;
use super::types::HandleId;
use super::types::InvocationMode;
use super::types::NodeError;
use super::types::{RawCancelCallback, RawGoalCallback};

// ============================================================================
// Action server registration
// ============================================================================

impl<S: Session, const MAX_CBS: usize, const CB_ARENA: usize> Executor<S, MAX_CBS, CB_ARENA> {
    /// Register an action server with goal/cancel callbacks.
    ///
    /// The executor automatically dispatches:
    /// - Goal acceptance via `goal_callback`
    /// - Cancel requests via `cancel_callback`
    /// - Result serving for completed goals
    ///
    /// Use the returned [`ActionServerHandle`] to publish feedback and complete goals.
    ///
    /// Uses default buffer sizes (1024 bytes) and max 4 concurrent goals.
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
        S::ServiceServerHandle: ServiceServerTrait,
        S::PublisherHandle: Publisher,
    {
        self.add_action_server_sized::<A, GoalF, CancelF, 1024, 1024, 1024, 4>(
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
        S::ServiceServerHandle: ServiceServerTrait,
        S::PublisherHandle: Publisher,
    {
        type Entry<
            A,
            Srv,
            Pub,
            GoalF,
            CancelF,
            const GB: usize,
            const RB: usize,
            const FB: usize,
            const MG: usize,
        > = ActionServerArenaEntry<A, Srv, Pub, GoalF, CancelF, GB, RB, FB, MG>;

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

        let offset = self.arena_alloc::<Entry<
            A,
            S::ServiceServerHandle,
            S::PublisherHandle,
            GoalF,
            CancelF,
            GOAL_BUF,
            RESULT_BUF,
            FEEDBACK_BUF,
            MAX_GOALS,
        >>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset)
                as *mut Entry<
                    A,
                    S::ServiceServerHandle,
                    S::PublisherHandle,
                    GoalF,
                    CancelF,
                    GOAL_BUF,
                    RESULT_BUF,
                    FEEDBACK_BUF,
                    MAX_GOALS,
                >;
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
                S::ServiceServerHandle,
                S::PublisherHandle,
                GoalF,
                CancelF,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            drop_fn: drop_entry::<
                Entry<
                    A,
                    S::ServiceServerHandle,
                    S::PublisherHandle,
                    GoalF,
                    CancelF,
                    GOAL_BUF,
                    RESULT_BUF,
                    FEEDBACK_BUF,
                    MAX_GOALS,
                >,
            >,
        });

        Ok(ActionServerHandle {
            entry_index: slot,
            publish_feedback_fn: as_publish_feedback::<
                A,
                S::ServiceServerHandle,
                S::PublisherHandle,
                GoalF,
                CancelF,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            complete_goal_fn: as_complete_goal::<
                A,
                S::ServiceServerHandle,
                S::PublisherHandle,
                GoalF,
                CancelF,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            set_goal_status_fn: as_set_goal_status::<
                A,
                S::ServiceServerHandle,
                S::PublisherHandle,
                GoalF,
                CancelF,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            active_goal_count_fn: as_active_goal_count::<
                A,
                S::ServiceServerHandle,
                S::PublisherHandle,
                GoalF,
                CancelF,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            for_each_active_goal_fn: as_for_each_active_goal::<
                A,
                S::ServiceServerHandle,
                S::PublisherHandle,
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
    pub fn publish_feedback<S: Session, const MAX_CBS: usize, const CB_ARENA: usize>(
        &self,
        executor: &mut Executor<S, MAX_CBS, CB_ARENA>,
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

    /// Complete a goal with final status and result.
    pub fn complete_goal<S, const MAX_CBS: usize, const CB_ARENA: usize>(
        &self,
        executor: &mut Executor<S, MAX_CBS, CB_ARENA>,
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

    /// Update a goal's status.
    pub fn set_goal_status<S, const MAX_CBS: usize, const CB_ARENA: usize>(
        &self,
        executor: &mut Executor<S, MAX_CBS, CB_ARENA>,
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

    /// Get the number of active goals.
    pub fn active_goal_count<S, const MAX_CBS: usize, const CB_ARENA: usize>(
        &self,
        executor: &Executor<S, MAX_CBS, CB_ARENA>,
    ) -> usize {
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

    /// Iterate over active goals.
    ///
    /// Calls `f` for each currently active goal.
    pub fn for_each_active_goal<S, const MAX_CBS: usize, const CB_ARENA: usize>(
        &self,
        executor: &Executor<S, MAX_CBS, CB_ARENA>,
        mut f: impl FnMut(&ActiveGoal<A>),
    ) {
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

impl<S: Session, const MAX_CBS: usize, const CB_ARENA: usize> Executor<S, MAX_CBS, CB_ARENA> {
    /// Register a raw action server with raw-bytes callbacks.
    ///
    /// Unlike [`add_action_server()`](Executor::add_action_server), this does
    /// not require `RosAction` — the goal/cancel callbacks receive raw CDR
    /// bytes. This is used by the C API thin wrapper.
    ///
    /// `type_name` and `type_hash` identify the action type for key expression
    /// construction and liveliness tokens.
    pub fn add_action_server_raw(
        &mut self,
        action_name: &str,
        type_name: &str,
        type_hash: &str,
        goal_callback: RawGoalCallback,
        cancel_callback: RawCancelCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<ActionServerRawHandle, NodeError>
    where
        S::ServiceServerHandle: ServiceServerTrait,
        S::PublisherHandle: Publisher,
    {
        self.add_action_server_raw_sized::<1024, 1024, 1024, 4>(
            action_name,
            type_name,
            type_hash,
            goal_callback,
            cancel_callback,
            context,
        )
    }

    /// Register a raw action server with custom buffer sizes.
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
        context: *mut core::ffi::c_void,
    ) -> Result<ActionServerRawHandle, NodeError>
    where
        S::ServiceServerHandle: ServiceServerTrait,
        S::PublisherHandle: Publisher,
    {
        type Entry<Srv, Pub, const GB: usize, const RB: usize, const FB: usize, const MG: usize> =
            ActionServerRawArenaEntry<Srv, Pub, GB, RB, FB, MG>;

        let slot = self.next_entry_slot()?;

        let action_info = ActionInfo::new(action_name, type_name, type_hash);

        let send_goal_keyexpr: heapless::String<256> = action_info.send_goal_key();
        let send_goal_info =
            ServiceInfo::new(&send_goal_keyexpr, type_name, type_hash).with_domain(0);
        let send_goal_server = self
            .session
            .create_service_server(&send_goal_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let cancel_goal_keyexpr: heapless::String<256> = action_info.cancel_goal_key();
        let cancel_goal_info = ServiceInfo::new(
            &cancel_goal_keyexpr,
            "action_msgs::srv::dds_::CancelGoal_",
            type_hash,
        )
        .with_domain(0);
        let cancel_goal_server = self
            .session
            .create_service_server(&cancel_goal_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let get_result_keyexpr: heapless::String<256> = action_info.get_result_key();
        let get_result_info =
            ServiceInfo::new(&get_result_keyexpr, type_name, type_hash).with_domain(0);
        let get_result_server = self
            .session
            .create_service_server(&get_result_info)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let feedback_keyexpr: heapless::String<256> = action_info.feedback_key();
        let feedback_topic = TopicInfo::new(&feedback_keyexpr, type_name, type_hash).with_domain(0);
        let feedback_publisher = self
            .session
            .create_publisher(&feedback_topic, QosSettings::BEST_EFFORT)
            .map_err(|_| NodeError::ActionCreationFailed)?;

        let status_keyexpr: heapless::String<256> = action_info.status_key();
        let status_topic = TopicInfo::new(
            &status_keyexpr,
            "action_msgs::msg::dds_::GoalStatusArray_",
            type_hash,
        )
        .with_domain(0);
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

        let offset = self.arena_alloc::<Entry<
            S::ServiceServerHandle,
            S::PublisherHandle,
            GOAL_BUF,
            RESULT_BUF,
            FEEDBACK_BUF,
            MAX_GOALS,
        >>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset)
                as *mut Entry<
                    S::ServiceServerHandle,
                    S::PublisherHandle,
                    GOAL_BUF,
                    RESULT_BUF,
                    FEEDBACK_BUF,
                    MAX_GOALS,
                >;
            core::ptr::write(
                entry_ptr,
                Entry {
                    core,
                    goal_callback,
                    cancel_callback,
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
                S::ServiceServerHandle,
                S::PublisherHandle,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            drop_fn: drop_entry::<
                Entry<
                    S::ServiceServerHandle,
                    S::PublisherHandle,
                    GOAL_BUF,
                    RESULT_BUF,
                    FEEDBACK_BUF,
                    MAX_GOALS,
                >,
            >,
        });

        Ok(ActionServerRawHandle {
            entry_index: slot,
            publish_feedback_fn: as_raw_publish_feedback::<
                S::ServiceServerHandle,
                S::PublisherHandle,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            complete_goal_fn: as_raw_complete_goal::<
                S::ServiceServerHandle,
                S::PublisherHandle,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            set_goal_status_fn: as_raw_set_goal_status::<
                S::ServiceServerHandle,
                S::PublisherHandle,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            active_goal_count_fn: as_raw_active_goal_count::<
                S::ServiceServerHandle,
                S::PublisherHandle,
                GOAL_BUF,
                RESULT_BUF,
                FEEDBACK_BUF,
                MAX_GOALS,
            >,
            for_each_active_goal_fn: as_raw_for_each_active_goal::<
                S::ServiceServerHandle,
                S::PublisherHandle,
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

impl ActionServerRawHandle {
    /// Get the [`HandleId`] for this action server.
    pub fn handle_id(&self) -> HandleId {
        HandleId(self.entry_index)
    }

    /// Publish feedback with raw CDR bytes.
    pub fn publish_feedback_raw<S: Session, const MAX_CBS: usize, const CB_ARENA: usize>(
        &self,
        executor: &mut Executor<S, MAX_CBS, CB_ARENA>,
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

    /// Complete a goal with raw CDR result bytes.
    pub fn complete_goal_raw<S, const MAX_CBS: usize, const CB_ARENA: usize>(
        &self,
        executor: &mut Executor<S, MAX_CBS, CB_ARENA>,
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

    /// Update a goal's status.
    pub fn set_goal_status<S, const MAX_CBS: usize, const CB_ARENA: usize>(
        &self,
        executor: &mut Executor<S, MAX_CBS, CB_ARENA>,
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

    /// Get the number of active goals.
    pub fn active_goal_count<S, const MAX_CBS: usize, const CB_ARENA: usize>(
        &self,
        executor: &Executor<S, MAX_CBS, CB_ARENA>,
    ) -> usize {
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

    /// Iterate over active goals (raw).
    pub fn for_each_active_goal<S, const MAX_CBS: usize, const CB_ARENA: usize>(
        &self,
        executor: &Executor<S, MAX_CBS, CB_ARENA>,
        mut f: impl FnMut(&RawActiveGoal),
    ) {
        if let Some(meta) = executor.entries[self.entry_index].as_ref() {
            let arena_ptr = executor.arena.as_ptr() as *const u8;
            unsafe {
                let data_ptr = arena_ptr.add(meta.offset);
                (self.for_each_active_goal_fn)(data_ptr, &mut f);
            }
        }
    }
}
