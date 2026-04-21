//! ROS 2 Lifecycle Services (REP-2002)
//!
//! Surfaces a [`LifecyclePollingNodeCtx`] to ROS 2 tooling (`ros2 lifecycle
//! set|get|nodes|list`) by registering the five standard services under the
//! node's namespace:
//!
//! - `~/change_state` — trigger a transition (`ChangeState`)
//! - `~/get_state` — read the current state (`GetState`)
//! - `~/get_available_states` — list every lifecycle state (`GetAvailableStates`)
//! - `~/get_available_transitions` — transitions reachable from the current
//!   state (`GetAvailableTransitions`)
//! - `~/get_transition_graph` — the full transition table (`GetAvailableTransitions`)
//!
//! Only four service *types* are used — `~/get_available_transitions` and
//! `~/get_transition_graph` share the `GetAvailableTransitions` type, matching
//! the upstream `rclcpp_lifecycle` convention.

// Note: Module is gated by `#[cfg(feature = "lifecycle-services")]` in lib.rs.

extern crate alloc;
use alloc::boxed::Box;

use nros_core::lifecycle::{
    LifecycleState as InternalState, LifecycleTransition as InternalTransition, apply_transition,
    can_transition,
};

use crate::lifecycle::LifecyclePollingNodeCtx;

pub(crate) use nros_lifecycle_msgs::msg::{
    State as MsgState, Transition as MsgTransition, TransitionDescription as MsgTransitionDesc,
};
pub(crate) use nros_lifecycle_msgs::srv::{
    ChangeState, ChangeStateRequest, ChangeStateResponse, GetAvailableStates,
    GetAvailableStatesRequest, GetAvailableStatesResponse, GetAvailableTransitions,
    GetAvailableTransitionsRequest, GetAvailableTransitionsResponse, GetState, GetStateRequest,
    GetStateResponse,
};

// ═══════════════════════════════════════════════════════════════════════════
// Wire ID constants (match lifecycle_msgs/msg/State.msg + Transition.msg)
// ═══════════════════════════════════════════════════════════════════════════

/// Primary and transition state IDs defined by `lifecycle_msgs/State`.
pub mod state_id {
    pub const PRIMARY_STATE_UNKNOWN: u8 = 0;
    pub const PRIMARY_STATE_UNCONFIGURED: u8 = 1;
    pub const PRIMARY_STATE_INACTIVE: u8 = 2;
    pub const PRIMARY_STATE_ACTIVE: u8 = 3;
    pub const PRIMARY_STATE_FINALIZED: u8 = 4;
    pub const TRANSITION_STATE_ERRORPROCESSING: u8 = 15;
}

/// Publicly invocable transition IDs from `lifecycle_msgs/Transition`.
pub mod transition_id {
    pub const CREATE: u8 = 0;
    pub const CONFIGURE: u8 = 1;
    pub const CLEANUP: u8 = 2;
    pub const ACTIVATE: u8 = 3;
    pub const DEACTIVATE: u8 = 4;
    pub const UNCONFIGURED_SHUTDOWN: u8 = 5;
    pub const INACTIVE_SHUTDOWN: u8 = 6;
    pub const ACTIVE_SHUTDOWN: u8 = 7;
    pub const DESTROY: u8 = 8;
}

// ═══════════════════════════════════════════════════════════════════════════
// TYPE CONVERSIONS: Internal ↔ lifecycle_msgs
// ═══════════════════════════════════════════════════════════════════════════

/// Build a `lifecycle_msgs/State` from an internal state enum.
pub fn to_msg_state(state: InternalState) -> MsgState {
    let (id, label): (u8, &str) = match state {
        InternalState::Unconfigured => (state_id::PRIMARY_STATE_UNCONFIGURED, "unconfigured"),
        InternalState::Inactive => (state_id::PRIMARY_STATE_INACTIVE, "inactive"),
        InternalState::Active => (state_id::PRIMARY_STATE_ACTIVE, "active"),
        InternalState::Finalized => (state_id::PRIMARY_STATE_FINALIZED, "finalized"),
        InternalState::ErrorProcessing => {
            (state_id::TRANSITION_STATE_ERRORPROCESSING, "errorprocessing")
        }
    };
    let mut msg = MsgState::default();
    msg.id = id;
    let _ = msg.label.push_str(label);
    msg
}

/// Build a `lifecycle_msgs/Transition` from an internal transition enum.
pub fn to_msg_transition(t: InternalTransition) -> MsgTransition {
    let (id, label): (u8, &str) = match t {
        InternalTransition::Configure => (transition_id::CONFIGURE, "configure"),
        InternalTransition::Cleanup => (transition_id::CLEANUP, "cleanup"),
        InternalTransition::Activate => (transition_id::ACTIVATE, "activate"),
        InternalTransition::Deactivate => (transition_id::DEACTIVATE, "deactivate"),
        InternalTransition::ShutdownUnconfigured => {
            (transition_id::UNCONFIGURED_SHUTDOWN, "shutdown")
        }
        InternalTransition::ShutdownInactive => (transition_id::INACTIVE_SHUTDOWN, "shutdown"),
        InternalTransition::ShutdownActive => (transition_id::ACTIVE_SHUTDOWN, "shutdown"),
        // ErrorRecovery is an implicit transition in rclcpp_lifecycle — map
        // it to a reserved internal ID so it's still round-trippable.
        InternalTransition::ErrorRecovery => (60, "error_recovery"),
    };
    let mut msg = MsgTransition::default();
    msg.id = id;
    let _ = msg.label.push_str(label);
    msg
}

/// Map a wire `Transition.id` back to an internal transition, given the
/// current state (needed to disambiguate the three shutdown variants).
pub fn from_msg_transition_id(id: u8, current: InternalState) -> Option<InternalTransition> {
    match id {
        transition_id::CONFIGURE => Some(InternalTransition::Configure),
        transition_id::CLEANUP => Some(InternalTransition::Cleanup),
        transition_id::ACTIVATE => Some(InternalTransition::Activate),
        transition_id::DEACTIVATE => Some(InternalTransition::Deactivate),
        transition_id::UNCONFIGURED_SHUTDOWN => Some(InternalTransition::ShutdownUnconfigured),
        transition_id::INACTIVE_SHUTDOWN => Some(InternalTransition::ShutdownInactive),
        transition_id::ACTIVE_SHUTDOWN => Some(InternalTransition::ShutdownActive),
        // The catch-all "shutdown" id (UNCONFIGURED_SHUTDOWN == 5) is handled
        // above; rclcpp additionally accepts a generic `shutdown` matched by
        // label. With label-free requests, fall back to the current state.
        _ => match (id, current) {
            (transition_id::DESTROY, _) => None, // destroy is not supported here
            _ => None,
        },
    }
}

/// Map a wire `Transition.label` back to an internal transition. `"shutdown"`
/// resolves to the variant matching the current state, mirroring rclcpp.
pub fn from_msg_transition_label(label: &str, current: InternalState) -> Option<InternalTransition> {
    match label {
        "configure" => Some(InternalTransition::Configure),
        "cleanup" => Some(InternalTransition::Cleanup),
        "activate" => Some(InternalTransition::Activate),
        "deactivate" => Some(InternalTransition::Deactivate),
        "shutdown" => match current {
            InternalState::Unconfigured => Some(InternalTransition::ShutdownUnconfigured),
            InternalState::Inactive => Some(InternalTransition::ShutdownInactive),
            InternalState::Active => Some(InternalTransition::ShutdownActive),
            _ => None,
        },
        "error_recovery" => Some(InternalTransition::ErrorRecovery),
        _ => None,
    }
}

/// Every primary transition that can appear in a transition graph. The
/// three shutdown variants are listed separately so their `start_state`
/// differs (mirroring rclcpp's graph shape).
const ALL_TRANSITIONS: [InternalTransition; 8] = [
    InternalTransition::Configure,
    InternalTransition::Cleanup,
    InternalTransition::Activate,
    InternalTransition::Deactivate,
    InternalTransition::ShutdownUnconfigured,
    InternalTransition::ShutdownInactive,
    InternalTransition::ShutdownActive,
    InternalTransition::ErrorRecovery,
];

/// Primary states plus ErrorProcessing — every reachable lifecycle state.
const ALL_STATES: [InternalState; 5] = [
    InternalState::Unconfigured,
    InternalState::Inactive,
    InternalState::Active,
    InternalState::Finalized,
    InternalState::ErrorProcessing,
];

fn transition_start_state(t: InternalTransition) -> InternalState {
    match t {
        InternalTransition::Configure => InternalState::Unconfigured,
        InternalTransition::Cleanup => InternalState::Inactive,
        InternalTransition::Activate => InternalState::Inactive,
        InternalTransition::Deactivate => InternalState::Active,
        InternalTransition::ShutdownUnconfigured => InternalState::Unconfigured,
        InternalTransition::ShutdownInactive => InternalState::Inactive,
        InternalTransition::ShutdownActive => InternalState::Active,
        InternalTransition::ErrorRecovery => InternalState::ErrorProcessing,
    }
}

fn transition_goal_state(t: InternalTransition) -> InternalState {
    // Assume the callback succeeds — that's the "goal" state the service
    // advertises. If it fails, apply_transition() will route to ErrorProcessing
    // at runtime; that's orthogonal to the advertised graph.
    apply_transition(
        transition_start_state(t),
        t,
        nros_core::lifecycle::TransitionResult::Success,
    )
}

fn build_transition_desc(t: InternalTransition) -> MsgTransitionDesc {
    let mut desc = MsgTransitionDesc::default();
    desc.transition = to_msg_transition(t);
    desc.start_state = to_msg_state(transition_start_state(t));
    desc.goal_state = to_msg_state(transition_goal_state(t));
    desc
}

// ═══════════════════════════════════════════════════════════════════════════
// HANDLERS
// ═══════════════════════════════════════════════════════════════════════════

/// Handle `~/change_state`. Looks up the transition by id (falling back to
/// label on `0`), invokes the registered callback, and reports success.
///
/// # Safety
/// Invokes `LifecyclePollingNodeCtx::trigger_transition`, which calls a user
/// C callback via a raw function pointer. The caller must uphold the usual
/// `*mut c_void` context-lifetime invariants documented on the state machine.
pub unsafe fn handle_change_state(
    sm: &mut LifecyclePollingNodeCtx,
    request: &ChangeStateRequest,
) -> Box<ChangeStateResponse> {
    let mut response = Box::new(ChangeStateResponse::default());
    let current = sm.state();

    // Prefer the numeric id when set; fall back to the label (supports the
    // generic "shutdown" label from `ros2 lifecycle set <node> shutdown`).
    let transition = from_msg_transition_id(request.transition.id, current)
        .or_else(|| from_msg_transition_label(request.transition.label.as_str(), current));

    if let Some(t) = transition {
        // SAFETY: forwarded to the caller via this function's `unsafe` contract.
        let result = unsafe { sm.trigger_transition(t) };
        response.success = matches!(result, Ok(_));
    }

    response
}

/// Handle `~/get_state`. Pure read — no state mutation.
pub fn handle_get_state(
    sm: &LifecyclePollingNodeCtx,
    _request: &GetStateRequest,
) -> Box<GetStateResponse> {
    let mut response = Box::new(GetStateResponse::default());
    response.current_state = to_msg_state(sm.state());
    response
}

/// Handle `~/get_available_states`. Returns every reachable state.
pub fn handle_get_available_states(
    _sm: &LifecyclePollingNodeCtx,
    _request: &GetAvailableStatesRequest,
) -> Box<GetAvailableStatesResponse> {
    let mut response = Box::new(GetAvailableStatesResponse::default());
    for state in ALL_STATES.iter().copied() {
        let _ = response.available_states.push(to_msg_state(state));
    }
    response
}

/// Handle `~/get_available_transitions`. Returns only the transitions that
/// are valid from the current state.
pub fn handle_get_available_transitions(
    sm: &LifecyclePollingNodeCtx,
    _request: &GetAvailableTransitionsRequest,
) -> Box<GetAvailableTransitionsResponse> {
    let mut response = Box::new(GetAvailableTransitionsResponse::default());
    let current = sm.state();
    for t in ALL_TRANSITIONS.iter().copied() {
        if transition_start_state(t) == current && can_transition(current, t) {
            let _ = response.available_transitions.push(build_transition_desc(t));
        }
    }
    response
}

/// Handle `~/get_transition_graph`. Returns the full static transition
/// table, regardless of the current state.
pub fn handle_get_transition_graph(
    _sm: &LifecyclePollingNodeCtx,
    _request: &GetAvailableTransitionsRequest,
) -> Box<GetAvailableTransitionsResponse> {
    let mut response = Box::new(GetAvailableTransitionsResponse::default());
    for t in ALL_TRANSITIONS.iter().copied() {
        let _ = response.available_transitions.push(build_transition_desc(t));
    }
    response
}

// ═══════════════════════════════════════════════════════════════════════════
// SERVICE SERVERS
// ═══════════════════════════════════════════════════════════════════════════

use crate::executor::{EmbeddedServiceServer, NodeError};

// LIFECYCLE_SERVICE_BUFFER_SIZE sits alongside PARAM_SERVICE_BUFFER_SIZE; the
// lifecycle payloads are far smaller, but reusing the same tuning knob keeps
// the build surface simple.
pub use crate::config::PARAM_SERVICE_BUFFER_SIZE as LIFECYCLE_SERVICE_BUFFER_SIZE;

type LcSrv<Svc> =
    EmbeddedServiceServer<Svc, LIFECYCLE_SERVICE_BUFFER_SIZE, LIFECYCLE_SERVICE_BUFFER_SIZE>;

/// Holds the five REP-2002 lifecycle service servers for a node.
///
/// Boxed when stored inside the executor to keep 5 × buffer_size out of
/// stack frames (same argument as `ParameterServiceServers`).
pub struct LifecycleServiceServers {
    change_state: LcSrv<ChangeState>,
    get_state: LcSrv<GetState>,
    get_available_states: LcSrv<GetAvailableStates>,
    get_available_transitions: LcSrv<GetAvailableTransitions>,
    get_transition_graph: LcSrv<GetAvailableTransitions>,
}

impl LifecycleServiceServers {
    pub(crate) fn new(
        change_state: LcSrv<ChangeState>,
        get_state: LcSrv<GetState>,
        get_available_states: LcSrv<GetAvailableStates>,
        get_available_transitions: LcSrv<GetAvailableTransitions>,
        get_transition_graph: LcSrv<GetAvailableTransitions>,
    ) -> Self {
        Self {
            change_state,
            get_state,
            get_available_states,
            get_available_transitions,
            get_transition_graph,
        }
    }

    /// Process every lifecycle service server, handling at most one request
    /// per server per call. Returns the number of requests handled.
    ///
    /// Mirrors `ParameterServiceServers::process` — split-borrow pattern so
    /// the state machine can live outside the server set.
    ///
    /// # Safety
    /// `change_state` forwards to `handle_change_state`, which calls user
    /// callbacks through raw function pointers. See that function's safety
    /// note.
    pub(crate) unsafe fn process(
        &mut self,
        sm: &mut LifecyclePollingNodeCtx,
    ) -> Result<usize, NodeError> {
        let mut count = 0;

        if self.change_state.handle_request_boxed(|req| {
            // SAFETY: forwarded via this function's unsafe contract.
            unsafe { handle_change_state(sm, req) }
        })? {
            count += 1;
        }

        if self
            .get_state
            .handle_request_boxed(|req| handle_get_state(sm, req))?
        {
            count += 1;
        }

        if self
            .get_available_states
            .handle_request_boxed(|req| handle_get_available_states(sm, req))?
        {
            count += 1;
        }

        if self
            .get_available_transitions
            .handle_request_boxed(|req| handle_get_available_transitions(sm, req))?
        {
            count += 1;
        }

        if self
            .get_transition_graph
            .handle_request_boxed(|req| handle_get_transition_graph(sm, req))?
        {
            count += 1;
        }

        Ok(count)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TYPE-ERASED PROCESSING (for Executor integration)
// ═══════════════════════════════════════════════════════════════════════════

/// Type-erased trait so the executor can call `process` without knowing the
/// concrete server set.
///
/// # Safety
/// `process_services` has the same contract as
/// [`LifecycleServiceServers::process`].
pub(crate) trait LifecycleServiceProcessor {
    unsafe fn process_services(
        &mut self,
        sm: &mut LifecyclePollingNodeCtx,
    ) -> Result<usize, NodeError>;
}

impl LifecycleServiceProcessor for LifecycleServiceServers {
    unsafe fn process_services(
        &mut self,
        sm: &mut LifecyclePollingNodeCtx,
    ) -> Result<usize, NodeError> {
        // SAFETY: forwarded via this trait method's contract.
        unsafe { self.process(sm) }
    }
}

/// Pairs the state machine with its registered service servers. Stored on
/// the executor (outside the callback arena) when lifecycle services are
/// registered — analogous to `ParamState`.
pub(crate) struct LifecycleRuntimeState {
    pub(crate) state_machine: LifecyclePollingNodeCtx,
    pub(crate) services: Box<dyn LifecycleServiceProcessor>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_roundtrip() {
        let msg = to_msg_state(InternalState::Active);
        assert_eq!(msg.id, state_id::PRIMARY_STATE_ACTIVE);
        assert_eq!(msg.label.as_str(), "active");
    }

    #[test]
    fn transition_roundtrip_by_id() {
        let msg = to_msg_transition(InternalTransition::Configure);
        assert_eq!(msg.id, transition_id::CONFIGURE);
        assert_eq!(
            from_msg_transition_id(msg.id, InternalState::Unconfigured),
            Some(InternalTransition::Configure)
        );
    }

    #[test]
    fn shutdown_label_picks_variant_by_state() {
        assert_eq!(
            from_msg_transition_label("shutdown", InternalState::Inactive),
            Some(InternalTransition::ShutdownInactive)
        );
        assert_eq!(
            from_msg_transition_label("shutdown", InternalState::Active),
            Some(InternalTransition::ShutdownActive)
        );
    }

    #[test]
    fn get_state_handler_reports_unconfigured() {
        let sm = LifecyclePollingNodeCtx::new();
        let req = GetStateRequest::default();
        let resp = handle_get_state(&sm, &req);
        assert_eq!(resp.current_state.id, state_id::PRIMARY_STATE_UNCONFIGURED);
    }

    #[test]
    fn get_available_states_has_five() {
        let sm = LifecyclePollingNodeCtx::new();
        let req = GetAvailableStatesRequest::default();
        let resp = handle_get_available_states(&sm, &req);
        assert_eq!(resp.available_states.len(), 5);
    }

    #[test]
    fn get_available_transitions_from_unconfigured() {
        let sm = LifecyclePollingNodeCtx::new();
        let req = GetAvailableTransitionsRequest::default();
        let resp = handle_get_available_transitions(&sm, &req);
        // From Unconfigured, only Configure and ShutdownUnconfigured are valid.
        assert_eq!(resp.available_transitions.len(), 2);
        let ids: heapless::Vec<u8, 8> = resp
            .available_transitions
            .iter()
            .map(|d| d.transition.id)
            .collect();
        assert!(ids.contains(&transition_id::CONFIGURE));
        assert!(ids.contains(&transition_id::UNCONFIGURED_SHUTDOWN));
    }

    #[test]
    fn get_transition_graph_lists_all() {
        let sm = LifecyclePollingNodeCtx::new();
        let req = GetAvailableTransitionsRequest::default();
        let resp = handle_get_transition_graph(&sm, &req);
        assert_eq!(resp.available_transitions.len(), ALL_TRANSITIONS.len());
    }

    #[test]
    fn change_state_with_no_callback_reaches_inactive() {
        let mut sm = LifecyclePollingNodeCtx::new();
        let mut req = ChangeStateRequest::default();
        req.transition.id = transition_id::CONFIGURE;

        // SAFETY: no callback registered; trigger_transition falls back to
        // TransitionResult::Success without calling a null pointer.
        let resp = unsafe { handle_change_state(&mut sm, &req) };
        assert!(resp.success);
        assert_eq!(sm.state(), InternalState::Inactive);
    }
}
