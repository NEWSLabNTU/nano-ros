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
    LifecycleState as InternalState, LifecycleTransition as InternalTransition, can_transition,
};

use crate::lifecycle::LifecyclePollingNodeCtx;

pub(crate) use nros_lifecycle_msgs::{
    msg::{State as MsgState, Transition as MsgTransition},
    srv::{ChangeState, GetAvailableStates, GetAvailableTransitions, GetState},
};

// phase-382 W1' — the request/reply VALUE types are needed only by the
// by-value oracle and the round-trip tests that compare against it. The
// streaming handlers never name them, which is the point: a
// `ChangeStateRequest` is 272 bytes of `heapless::String<256>` that used to
// land on the calling task's stack once per `ros2 lifecycle set`.
#[cfg(test)]
pub(crate) use nros_lifecycle_msgs::{
    msg::TransitionDescription as MsgTransitionDesc,
    srv::{
        ChangeStateRequest, ChangeStateResponse, GetAvailableStatesRequest,
        GetAvailableStatesResponse, GetAvailableTransitionsRequest,
        GetAvailableTransitionsResponse, GetStateRequest, GetStateResponse,
    },
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
///
/// Issue 1099 — since `nros_core::lifecycle::LifecycleTransition` carries these
/// same numbers as its discriminants, this module is no longer a TRANSLATION
/// table: `transition_wire` and `from_msg_transition_id` are identity, and
/// `transition_ids_match_enum_discriminants` below pins that. Keep the
/// constants anyway; they are the names the wire code reads by, and they are
/// what makes the two ids we do NOT implement (`CREATE`, `DESTROY`) visible.
pub mod transition_id {
    /// Not implemented: a node is constructed by the arena, not by a transition.
    pub const CREATE: u8 = 0;
    pub const CONFIGURE: u8 = 1;
    pub const CLEANUP: u8 = 2;
    pub const ACTIVATE: u8 = 3;
    pub const DEACTIVATE: u8 = 4;
    pub const UNCONFIGURED_SHUTDOWN: u8 = 5;
    pub const INACTIVE_SHUTDOWN: u8 = 6;
    pub const ACTIVE_SHUTDOWN: u8 = 7;
    /// Not implemented: nothing is deallocated in a fixed arena.
    pub const DESTROY: u8 = 8;
    /// `TRANSITION_ON_ERROR_SUCCESS` — upstream's implicit
    /// `ErrorProcessing -> Unconfigured` edge, which is what our explicit
    /// `ErrorRecovery` is.
    pub const ERROR_RECOVERY: u8 = 60;
}

// ═══════════════════════════════════════════════════════════════════════════
// TYPE CONVERSIONS: Internal ↔ lifecycle_msgs
// ═══════════════════════════════════════════════════════════════════════════

/// The `(id, label)` pair a `lifecycle_msgs/State` carries on the wire.
///
/// phase-382 W1' — ONE source of truth for the labels, shared by the by-value
/// [`to_msg_state`] and by the streaming writer `write_state`, so the two can
/// never disagree about what goes out.
pub(crate) fn state_wire(state: InternalState) -> (u8, &'static str) {
    // phase-417 W4.f — the LABEL comes from the enum and the ID from the
    // `lifecycle_msgs` constant, deliberately. The label used to be spelled
    // here and nowhere else, so neither C nor C++ could read a state's label
    // at all and giving them one meant a fourth set of literals. The id stays
    // a `state_id::*` reference because that is what pins our discriminants to
    // upstream's numbering (issue 1099) — collapsing it to `state as u8` would
    // make the assertion assert itself.
    let id = match state {
        InternalState::Unconfigured => state_id::PRIMARY_STATE_UNCONFIGURED,
        InternalState::Inactive => state_id::PRIMARY_STATE_INACTIVE,
        InternalState::Active => state_id::PRIMARY_STATE_ACTIVE,
        InternalState::Finalized => state_id::PRIMARY_STATE_FINALIZED,
        InternalState::ErrorProcessing => state_id::TRANSITION_STATE_ERRORPROCESSING,
    };
    (id, state.label())
}

/// The `(id, label)` pair a `lifecycle_msgs/Transition` carries on the wire.
/// Companion to [`state_wire`]; see the note there.
pub(crate) fn transition_wire(t: InternalTransition) -> (u8, &'static str) {
    // See `state_wire`: label from the enum, id from `lifecycle_msgs`.
    let id = match t {
        InternalTransition::Configure => transition_id::CONFIGURE,
        InternalTransition::Cleanup => transition_id::CLEANUP,
        InternalTransition::Activate => transition_id::ACTIVATE,
        InternalTransition::Deactivate => transition_id::DEACTIVATE,
        InternalTransition::ShutdownUnconfigured => transition_id::UNCONFIGURED_SHUTDOWN,
        InternalTransition::ShutdownInactive => transition_id::INACTIVE_SHUTDOWN,
        InternalTransition::ShutdownActive => transition_id::ACTIVE_SHUTDOWN,
        // ErrorRecovery is an implicit transition in rclcpp_lifecycle; 60 is
        // upstream's `TRANSITION_ON_ERROR_SUCCESS`. Since issue 1099 that is
        // also the enum's own discriminant, so this row is identity like the
        // rest — `transition_id::ERROR_RECOVERY` names it.
        InternalTransition::ErrorRecovery => transition_id::ERROR_RECOVERY,
    };
    (id, t.label())
}

/// Build a `lifecycle_msgs/State` from an internal state enum.
pub fn to_msg_state(state: InternalState) -> MsgState {
    let (id, label) = state_wire(state);
    let mut msg = MsgState {
        id,
        ..Default::default()
    };
    // Phase 192.1/B — labels are a fixed, short, closed set (all fit today);
    // debug_assert so a future capacity regression surfaces loudly in tests
    // instead of silently truncating the label.
    //
    // **Do not wrap `push_str` itself in `debug_assert!`** — that macro
    // skips its entire expression in release builds, so the label would
    // never be written. Capture the result first, then assert.
    let r = msg.label.push_str(label);
    debug_assert!(
        r.is_ok(),
        "lifecycle label exceeds the bounded msg.label capacity"
    );
    msg
}

/// Build a `lifecycle_msgs/Transition` from an internal transition enum.
pub fn to_msg_transition(t: InternalTransition) -> MsgTransition {
    let (id, label) = transition_wire(t);
    let mut msg = MsgTransition {
        id,
        ..Default::default()
    };
    // Phase 192.1/B — see comment in `to_msg_state` for the
    // `debug_assert!` / `push_str` ordering bug we avoid here.
    let r = msg.label.push_str(label);
    debug_assert!(
        r.is_ok(),
        "lifecycle label exceeds the bounded msg.label capacity"
    );
    msg
}

/// Map a wire `Transition.id` back to an internal transition, given the
/// current state (needed to disambiguate the three shutdown variants).
pub fn from_msg_transition_id(id: u8, current: InternalState) -> Option<InternalTransition> {
    // `current` is unused: every id in `lifecycle_msgs/Transition` names its own
    // source state, so nothing here is ambiguous. Only the generic `"shutdown"`
    // LABEL is, and that is `from_msg_transition_label`'s job. The parameter
    // stays for symmetry with that function and for callers that pass both.
    let _ = current;
    // Issue 1099 — this is now exactly `LifecycleTransition::from_u8`, because
    // the enum's discriminants ARE these ids. Delegating rather than
    // re-listing them means the wire path cannot drift from the C/C++/Rust
    // paths again, which is how the ids came to disagree in the first place:
    // this function translated and nothing else did.
    //
    // `CREATE` (0) and `DESTROY` (8) are unimplemented and fall out as `None`.
    InternalTransition::from_u8(id)
}

/// Map a wire `Transition.label` back to an internal transition. `"shutdown"`
/// resolves to the variant matching the current state, mirroring rclcpp.
pub fn from_msg_transition_label(
    label: &str,
    current: InternalState,
) -> Option<InternalTransition> {
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

/// Every primary transition that can appear in a transition graph.
///
/// phase-417 W4.f — an ALIAS for `nros_core::lifecycle::LifecycleTransition::ALL`,
/// not a second table. This list, its start states and its goal states were all
/// private to this module, so the graph a remote peer could read over
/// `~/get_transition_graph` was unreadable to the node's own code in every one
/// of our three languages. Moving the table down to `nros-core` is what lets
/// the C and C++ accessors read the SAME eight rows rather than a copy.
const ALL_TRANSITIONS: [InternalTransition; 8] = InternalTransition::ALL;

/// Primary states plus ErrorProcessing — every reachable lifecycle state.
/// Aliases `nros_core::lifecycle::LifecycleState::ALL`; see `ALL_TRANSITIONS`.
const ALL_STATES: [InternalState; 5] = InternalState::ALL;

fn transition_start_state(t: InternalTransition) -> InternalState {
    t.source_state()
}

fn transition_goal_state(t: InternalTransition) -> InternalState {
    // Assume the callback succeeds — that's the "goal" state the service
    // advertises. If it fails, apply_transition() will route to ErrorProcessing
    // at runtime; that's orthogonal to the advertised graph.
    t.goal_state()
}

// ═══════════════════════════════════════════════════════════════════════════
// WIRE WRITERS — hand-written CDR that MIRRORS the generated `Serialize`
// ═══════════════════════════════════════════════════════════════════════════
//
// phase-382 W1'. The handlers below STREAM: they read the request's fields
// straight off the wire and write the reply's fields straight back, so neither
// side is ever materialised as a value. See the doc comment on
// `nros_rmw::ServiceTrait::handle_request_raw` for the contract.
//
// No `lifecycle_msgs` type uses a DHEADER — every generated `serialize` is
// plain sequential CDR, verified field-for-field against
// `packages/interfaces/generated/*/nros-lifecycle-msgs/src/`:
//
//   msg/State                 = write_u8(id),  write_string(label)
//   msg/Transition            = write_u8(id),  write_string(label)
//   msg/TransitionDescription = Transition, State start, State goal
//   srv/ChangeState             req = Transition  reply = write_bool(success)
//   srv/GetState                req = ()          reply = State
//   srv/GetAvailableStates      req = ()  reply = write_u32(len), State*
//   srv/GetAvailableTransitions req = ()  reply = write_u32(len), TransitionDescription*
//
// The generated impls spell the sequence length `write_u32(len as u32)`;
// `write_sequence_len` IS that call (cdr.rs), so the two agree byte for byte.
// `check_wire_writers_match_generated_serialize` in the tests below re-proves
// it on every build rather than on this comment's say-so.

use nros_core::{CdrReader, CdrWriter, SerError};
use nros_rmw::TransportError;

#[inline]
fn ser_err(_e: SerError) -> TransportError {
    TransportError::SerializationError
}

#[inline]
fn deser_err<E>(_e: E) -> TransportError {
    TransportError::DeserializationError
}

/// Write the `u8 id, string label` body shared by `lifecycle_msgs/State` and
/// `lifecycle_msgs/Transition` — the two messages have an identical field
/// shape, and both generated `serialize` impls are exactly these two calls.
#[inline]
fn write_id_label(w: &mut CdrWriter<'_>, id: u8, label: &str) -> Result<(), SerError> {
    w.write_u8(id)?;
    w.write_string(label)
}

/// Stream a `lifecycle_msgs/State` for `state`. No scratch: the label is a
/// `&'static str` out of [`state_wire`].
#[inline]
fn write_state(w: &mut CdrWriter<'_>, state: InternalState) -> Result<(), SerError> {
    let (id, label) = state_wire(state);
    write_id_label(w, id, label)
}

/// Stream a `lifecycle_msgs/Transition` for `t`.
#[inline]
fn write_transition(w: &mut CdrWriter<'_>, t: InternalTransition) -> Result<(), SerError> {
    let (id, label) = transition_wire(t);
    write_id_label(w, id, label)
}

/// Stream a `lifecycle_msgs/TransitionDescription` for `t` — transition, then
/// start state, then goal state, matching the generated impl's field order.
fn write_transition_desc(w: &mut CdrWriter<'_>, t: InternalTransition) -> Result<(), SerError> {
    write_transition(w, t)?;
    write_state(w, transition_start_state(t))?;
    write_state(w, transition_goal_state(t))
}

/// Stream a `lifecycle_msgs/TransitionEvent` — timestamp, transition, start
/// state, goal state, matching the generated impl's field order.
///
/// Issue 1587. The by-value type is three `heapless::String<256>` (one label
/// per nested `State`/`Transition`) plus a `u64`, i.e. ~800 bytes that would
/// land on the calling task's stack once per transition. Every other wire
/// writer in this module exists for that reason, and this one reuses two of
/// them unchanged.
fn write_transition_event(
    w: &mut CdrWriter<'_>,
    timestamp_ns: u64,
    transition: InternalTransition,
    start_state: InternalState,
    goal_state: InternalState,
) -> Result<(), SerError> {
    w.write_u64(timestamp_ns)?;
    write_transition(w, transition)?;
    write_state(w, start_state)?;
    write_state(w, goal_state)
}

/// The publish buffer one `TransitionEvent` needs in the worst case.
///
/// Derived from the message rather than inherited from
/// `DEFAULT_RX_BUF_SIZE`, for the reason [`crate::rosout::TX_BUF`] gives: that
/// default is whatever the IMAGE declared through
/// `NROS_SUBSCRIPTION_BUFFER_SIZE`, a bound over the types that image's own
/// endpoints carry, and `~/transition_event` is not one of them.
///
/// The three nested labels are `&'static str`s out of [`state_wire`] and
/// [`transition_wire`], so the true worst case is bounded by the longest of
/// those; `a_maximal_transition_event_fits_the_derived_buffer` measures it
/// against this number rather than taking the arithmetic on trust.
const TRANSITION_EVENT_TX_BUF: usize = 4   // CDR encapsulation header
    + 4                                    // XCDR2 DHEADER
    + 8                                    // uint64 timestamp
    + 3 * (4 + 4 + 4 + 32 + 4)             // Transition + 2x State: DHEADER, id+pad, len, label, pad
    + 4; // DHEADER close

/// Is `t` reachable from `current`? Used twice per
/// `get_available_transitions` call — once to establish the sequence length,
/// once to emit the elements — because CDR puts the length first.
#[inline]
fn transition_available_from(t: InternalTransition, current: InternalState) -> bool {
    transition_start_state(t) == current && can_transition(current, t)
}

// ═══════════════════════════════════════════════════════════════════════════
// STREAMING HANDLERS
// ═══════════════════════════════════════════════════════════════════════════

/// Handle `~/change_state`. Looks up the transition by id (falling back to
/// label), invokes the registered callback, and reports success.
///
/// Streams: the request's `Transition` is read field by field and the label is
/// BORROWED out of the request buffer (`CdrReader::read_string` is zero-copy),
/// so no `ChangeStateRequest` — 272 bytes of it a `heapless::String<256>` —
/// is ever built.
///
/// # Safety
/// Invokes `LifecyclePollingNodeCtx::trigger_transition`, which calls a user
/// C callback via a raw function pointer. The caller must uphold the usual
/// `*mut c_void` context-lifetime invariants documented on the state machine.
pub(crate) unsafe fn stream_change_state(
    sm: &mut LifecyclePollingNodeCtx,
    reader: &mut CdrReader<'_>,
    writer: &mut CdrWriter<'_>,
) -> Result<(), TransportError> {
    // ChangeStateRequest = Transition { u8 id, string label }.
    let id = reader.read_u8().map_err(deser_err)?;
    let label = reader.read_string().map_err(deser_err)?;

    let current = sm.state();

    // Prefer the numeric id when set; fall back to the label (supports the
    // generic "shutdown" label from `ros2 lifecycle set <node> shutdown`).
    let transition =
        from_msg_transition_id(id, current).or_else(|| from_msg_transition_label(label, current));

    let success = match transition {
        // SAFETY: forwarded to the caller via this function's `unsafe` contract.
        Some(t) => unsafe { sm.trigger_transition(t) }.is_ok(),
        None => false,
    };

    // ChangeStateResponse = bool success.
    writer.write_bool(success).map_err(ser_err)
}

/// Handle `~/get_state`. Pure read — no state mutation. The request is empty,
/// so nothing is read.
pub(crate) fn stream_get_state(
    sm: &LifecyclePollingNodeCtx,
    _reader: &mut CdrReader<'_>,
    writer: &mut CdrWriter<'_>,
) -> Result<(), TransportError> {
    write_state(writer, sm.state()).map_err(ser_err)
}

/// Handle `~/get_available_states`. Streams every reachable state straight out
/// of the `ALL_STATES` table — the length is `ALL_STATES.len()`, known before
/// the first element, so nothing is buffered.
pub(crate) fn stream_get_available_states(
    _sm: &LifecyclePollingNodeCtx,
    _reader: &mut CdrReader<'_>,
    writer: &mut CdrWriter<'_>,
) -> Result<(), TransportError> {
    writer
        .write_sequence_len(ALL_STATES.len())
        .map_err(ser_err)?;
    for state in ALL_STATES {
        write_state(writer, state).map_err(ser_err)?;
    }
    Ok(())
}

/// Handle `~/get_available_transitions`. Returns only the transitions valid
/// from the current state.
///
/// CDR puts the sequence length before the elements, so the eight-entry
/// `ALL_TRANSITIONS` table is walked twice — once to count, once to emit.
/// A second pass over a const table is cheaper than any scratch buffer that
/// would let us do it in one.
pub(crate) fn stream_get_available_transitions(
    sm: &LifecyclePollingNodeCtx,
    _reader: &mut CdrReader<'_>,
    writer: &mut CdrWriter<'_>,
) -> Result<(), TransportError> {
    let current = sm.state();

    let count = ALL_TRANSITIONS
        .iter()
        .filter(|&&t| transition_available_from(t, current))
        .count();
    writer.write_sequence_len(count).map_err(ser_err)?;

    for t in ALL_TRANSITIONS {
        if transition_available_from(t, current) {
            write_transition_desc(writer, t).map_err(ser_err)?;
        }
    }
    Ok(())
}

/// Handle `~/get_transition_graph`. Streams the full static transition table,
/// regardless of the current state.
pub(crate) fn stream_get_transition_graph(
    _sm: &LifecyclePollingNodeCtx,
    _reader: &mut CdrReader<'_>,
    writer: &mut CdrWriter<'_>,
) -> Result<(), TransportError> {
    writer
        .write_sequence_len(ALL_TRANSITIONS.len())
        .map_err(ser_err)?;
    for t in ALL_TRANSITIONS {
        write_transition_desc(writer, t).map_err(ser_err)?;
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// BY-VALUE HANDLERS — TEST ORACLE ONLY (phase-382 W1')
// ═══════════════════════════════════════════════════════════════════════════
//
// These are the handlers that used to be dispatched, via
// `handle_request_boxed`. They are kept — not deleted — because they are the
// only independent statement of what these five services are supposed to put
// on the wire: they build the GENERATED reply type and let the GENERATED
// `Serialize` encode it. The round-trip tests below run them beside the
// streaming handlers and compare, which is what stops the hand-written CDR
// above from drifting.
//
// Nothing outside `#[cfg(test)]` may call them: they allocate, and removing
// that allocation is the whole point of the streaming path.

#[cfg(test)]
fn build_transition_desc(t: InternalTransition) -> MsgTransitionDesc {
    MsgTransitionDesc {
        transition: to_msg_transition(t),
        start_state: to_msg_state(transition_start_state(t)),
        goal_state: to_msg_state(transition_goal_state(t)),
    }
}

/// Oracle for [`stream_change_state`].
///
/// # Safety
/// Same contract as [`stream_change_state`].
#[cfg(test)]
pub(crate) unsafe fn handle_change_state(
    sm: &mut LifecyclePollingNodeCtx,
    request: &ChangeStateRequest,
) -> Box<ChangeStateResponse> {
    let mut response = Box::new(ChangeStateResponse::default());
    let current = sm.state();

    let transition = from_msg_transition_id(request.transition.id, current)
        .or_else(|| from_msg_transition_label(request.transition.label.as_str(), current));

    if let Some(t) = transition {
        // SAFETY: forwarded to the caller via this function's `unsafe` contract.
        response.success = unsafe { sm.trigger_transition(t) }.is_ok();
    }

    response
}

/// Oracle for [`stream_get_state`].
#[cfg(test)]
pub(crate) fn handle_get_state(
    sm: &LifecyclePollingNodeCtx,
    _request: &GetStateRequest,
) -> Box<GetStateResponse> {
    let mut response = Box::new(GetStateResponse::default());
    response.current_state = to_msg_state(sm.state());
    response
}

/// Oracle for [`stream_get_available_states`].
#[cfg(test)]
pub(crate) fn handle_get_available_states(
    _sm: &LifecyclePollingNodeCtx,
    _request: &GetAvailableStatesRequest,
) -> Box<GetAvailableStatesResponse> {
    let mut response = Box::new(GetAvailableStatesResponse::default());
    for state in ALL_STATES.iter().copied() {
        let r = response.available_states.push(to_msg_state(state));
        debug_assert!(r.is_ok(), "available_states exceeds bounded capacity");
    }
    response
}

/// Oracle for [`stream_get_available_transitions`].
#[cfg(test)]
pub(crate) fn handle_get_available_transitions(
    sm: &LifecyclePollingNodeCtx,
    _request: &GetAvailableTransitionsRequest,
) -> Box<GetAvailableTransitionsResponse> {
    let mut response = Box::new(GetAvailableTransitionsResponse::default());
    let current = sm.state();
    for t in ALL_TRANSITIONS.iter().copied() {
        if transition_start_state(t) == current && can_transition(current, t) {
            let _ = response
                .available_transitions
                .push(build_transition_desc(t));
        }
    }
    response
}

/// Oracle for [`stream_get_transition_graph`].
#[cfg(test)]
pub(crate) fn handle_get_transition_graph(
    _sm: &LifecyclePollingNodeCtx,
    _request: &GetAvailableTransitionsRequest,
) -> Box<GetAvailableTransitionsResponse> {
    let mut response = Box::new(GetAvailableTransitionsResponse::default());
    for t in ALL_TRANSITIONS.iter().copied() {
        let _ = response
            .available_transitions
            .push(build_transition_desc(t));
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

/// How many zenoh queryables the REP-2002 lifecycle services claim on a node.
///
/// Issue 0827 — FIVE, not six. `change_state`, `get_state`,
/// `get_available_states`, `get_available_transitions`, `get_transition_graph`.
/// Both places that stated a number for these stated six, which is where the
/// widely-quoted "twelve slots before the application declares anything" came
/// from; it is eleven. See `crate::parameter_services::PARAM_SERVICE_QUERYABLES`
/// for why the count lives here rather than in the RMW.
pub const LIFECYCLE_SERVICE_QUERYABLES: usize = 5;

/// The topic suffix REP-2002 names for the transition announcement.
///
/// Appended to the node's FQN exactly as the five service suffixes are, so
/// `~/transition_event` on a node `/lc_talker` is `/lc_talker/transition_event`
/// — which is the name `ros2 topic info` reports for an upstream lifecycle
/// node.
pub const TRANSITION_EVENT_SUFFIX: &str = "transition_event";

/// How many PUBLISHERS the REP-2002 lifecycle family claims on an executor.
///
/// Issue 1587 — ONE, the `~/transition_event` publisher. REP-2002's
/// communication interface is the five services PLUS this topic; shipping the
/// services alone meant a supervising node could not watch a managed node's
/// transitions and had to poll `~/get_state`.
///
/// # What it costs, and the expensive answer that MEASUREMENT refused
///
/// A transient-local publisher is also a zenoh QUERYABLE — it retains its last
/// sample and declares a cache queryable so a late joiner's history query can
/// reach it (issue 1378). `ZPICO_MAX_QUERYABLES` defaults to 8 on an embedded
/// build while `[param_services]` (6) and `[lifecycle]` (5) already claim
/// eleven between them (issue 0460), so a sixth lifecycle slot would be the
/// difference between an image that boots and one that dies in
/// `ServiceServerCreationFailed`. phase-467's study priced this row as the most
/// expensive of its thirteen on exactly that assumption, and told whoever took
/// it to decide the QoS first.
///
/// **It is not transient-local.** MEASURED on 2026-09-29 and again on
/// 2026-10-01 against a live `ros2 run lifecycle lifecycle_talker` on humble:
///
/// ```text
/// $ ros2 topic info -v /lc_talker/transition_event
///   Reliability: RELIABLE
///   Durability: VOLATILE
///   Lifespan: Infinite
/// ```
///
/// That is `rmw_qos_profile_default`, which `rcl_lifecycle`'s
/// `com_interface.c` takes from `rcl_publisher_get_default_options()` and
/// never overrides — i.e. [`QoSProfile::QOS_PROFILE_DEFAULT`], KEEP_LAST(10) /
/// RELIABLE / VOLATILE, byte for byte what this tree already has. So the term
/// this row adds is **one publisher slot and one liveliness token per
/// lifecycle-enabled image, and ZERO queryables** — nothing reaches
/// `ZPICO_MAX_QUERYABLES`, `MAX_TL_PUBLISHERS` or
/// `nros_sizing_descriptor::transient_local_publishers_over`. A test holds
/// that: `the_transition_event_publisher_is_volatile`.
///
/// [`QoSProfile::QOS_PROFILE_DEFAULT`]: nros_rmw::QoSProfile::QOS_PROFILE_DEFAULT
pub const LIFECYCLE_SERVICE_PUBLISHERS: usize = 1;

/// The QoS `~/transition_event` is published with: upstream's, unchanged.
///
/// Stated as a function rather than taken inline at the creation site so the
/// test above has something to assert on, the way `crate::rosout::qos` /
/// `qos_bounded` do. See [`LIFECYCLE_SERVICE_PUBLISHERS`] for the measurement.
#[must_use]
pub fn transition_event_qos() -> nros_rmw::QoSProfile {
    nros_rmw::QoSProfile::QOS_PROFILE_DEFAULT
}

// phase-461 W2 / issue 1352 -- the lifecycle family is the parameter family's
// traffic class: one request, one reply, from a client that waits, five
// queryables per node. So it brings its own inbox on the same terms and at the
// same geometry, the way it already shares the buffer-size alias above.
//
// The slot size is the parameter family's because the lifecycle payloads are
// strictly smaller (a transition id and a label against a set_parameters over
// every declared parameter), so the parameter bound is an upper bound here and
// a second knob would be a second number saying less. The DEPTH is the same 1
// and for the same reason: `ros2 lifecycle set` sends one request and waits.
//
// Issue 1468 -- from `crate::param_sizing`, not from `crate::parameter_services`.
// Same numbers, same argument; the difference is that the geometry sits below
// BOTH service features now, so reading it here no longer requires an image
// that wants lifecycle to also carry the parameter services and their message
// crate. Reading it from the gated module is what made this configuration --
// `lifecycle-services` without `param-services`, which is what
// `examples/native/rust/lifecycle-node` builds -- fail to compile at all.
pub use crate::param_sizing::{
    PARAM_INBOX_DEPTH as LIFECYCLE_INBOX_DEPTH,
    PARAM_INBOX_SLOT_BYTES as LIFECYCLE_INBOX_SLOT_BYTES,
};

/// How many rings the lifecycle family needs: one per queryable, over every
/// node one executor can serve.
///
/// ZERO while no backend the runtime can reach accepts a caller-owned ring --
/// see `crate::parameter_services::PARAM_INBOX_COUNT`, which states what
/// that is waiting on. (A name, not an intra-doc link: that module is behind
/// `param-services` and this one is not, so a link would dangle in exactly the
/// configuration issue 1468 was about.)
pub const LIFECYCLE_INBOX_COUNT: usize =
    if <crate::session::ConcreteSession as nros_rmw::Session>::SUPPORTS_CALLER_INBOX {
        LIFECYCLE_SERVICE_QUERYABLES * crate::param_sizing::MAX_SERVICE_SETS
    } else {
        0
    };

/// The lifecycle family's rings. `static`, not heap, for the reason
/// `crate::parameter_services::PARAM_INBOX` gives.
pub static LIFECYCLE_INBOX: [nros_rmw::CallerInboxStorage<
    LIFECYCLE_INBOX_SLOT_BYTES,
    LIFECYCLE_INBOX_DEPTH,
>; LIFECYCLE_INBOX_COUNT] = [const { nros_rmw::CallerInboxStorage::new() }; LIFECYCLE_INBOX_COUNT];

type LcSrv<Svc> =
    EmbeddedServiceServer<Svc, LIFECYCLE_SERVICE_BUFFER_SIZE, LIFECYCLE_SERVICE_BUFFER_SIZE>;

/// Holds the five REP-2002 lifecycle service servers for a node.
///
/// Boxed when stored inside the executor to keep 5 × buffer_size out of
/// stack frames. (The parameter family made the same argument until issue
/// 1270 moved its buffers into one pair per executor; these five still own
/// theirs -- one set per executor, not per node.)
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
    /// phase-382 W1' — every server STREAMS (`handle_request_raw`). Nothing
    /// here materialises a request or a reply, so this path needs no
    /// allocator and puts nothing bigger than a `(u8, &str)` on the stack.
    ///
    /// # Safety
    /// `change_state` forwards to `stream_change_state`, which calls user
    /// callbacks through raw function pointers. See that function's safety
    /// note.
    pub(crate) unsafe fn process(
        &mut self,
        sm: &mut LifecyclePollingNodeCtx,
    ) -> Result<usize, NodeError> {
        let mut count = 0;

        if self.change_state.handle_request_raw(|reader, writer| {
            // SAFETY: forwarded via this function's unsafe contract.
            unsafe { stream_change_state(sm, reader, writer) }
        })? {
            count += 1;
        }

        if self
            .get_state
            .handle_request_raw(|reader, writer| stream_get_state(sm, reader, writer))?
        {
            count += 1;
        }

        if self
            .get_available_states
            .handle_request_raw(|reader, writer| stream_get_available_states(sm, reader, writer))?
        {
            count += 1;
        }

        if self
            .get_available_transitions
            .handle_request_raw(|reader, writer| {
                stream_get_available_transitions(sm, reader, writer)
            })?
        {
            count += 1;
        }

        if self
            .get_transition_graph
            .handle_request_raw(|reader, writer| stream_get_transition_graph(sm, reader, writer))?
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

/// Pairs the state machine with its registered service servers and its
/// `~/transition_event` publisher. Stored on the executor (outside the
/// callback arena) when lifecycle services are registered — analogous to
/// `ParamState`.
pub(crate) struct LifecycleRuntimeState {
    pub(crate) state_machine: LifecyclePollingNodeCtx,
    pub(crate) services: Box<dyn LifecycleServiceProcessor>,
    /// Issue 1587 — REP-2002's announcement channel. Not an `Option`: an
    /// image that registered the lifecycle family has the slot budgeted for it
    /// (`InfraServices::publishers` in the entity inventory), so a publisher
    /// that could not be created is a sizing failure to report at
    /// registration, not a capability to silently drop.
    pub(crate) transition_event: crate::session::RmwPublisher,
}

impl LifecycleRuntimeState {
    /// Pump the five services, then announce whatever transitions ran.
    ///
    /// ONE method rather than two calls at each of `spin_once`'s two lifecycle
    /// sites: the announcement is not optional work a caller may skip, and the
    /// two sites were already a duplication that a third would have widened.
    ///
    /// # Safety
    /// Same contract as [`LifecycleServiceServers::process`] — `change_state`
    /// dispatches a user-supplied C callback through a raw function pointer.
    pub(crate) unsafe fn process(&mut self) -> Result<usize, NodeError> {
        // SAFETY: forwarded via this function's own unsafe contract.
        let handled = unsafe { self.services.process_services(&mut self.state_machine) };
        self.announce_transitions();
        handled
    }

    /// Drain the state machine's announcement queue onto `~/transition_event`.
    ///
    /// A transport error drops the record rather than failing the spin: the
    /// node's STATE is already what the services report, and refusing to spin
    /// because an announcement did not reach the wire would turn a missed
    /// notification into a stopped node.
    fn announce_transitions(&mut self) {
        use nros_rmw::Publisher as _;

        while let Some(ev) = self.state_machine.take_announcement() {
            let mut buf = [0u8; TRANSITION_EVENT_TX_BUF];
            let Ok(mut w) = crate::tx_writer(&mut buf) else {
                continue;
            };
            if write_transition_event(
                &mut w,
                ev.timestamp_ns,
                ev.transition,
                ev.start_state,
                ev.goal_state,
            )
            .is_err()
            {
                continue;
            }
            let len = w.position();
            let _ = self.transition_event.publish_raw(&buf[..len]);
        }

        // A bounded queue that discards without counting is the defect
        // `zpico_session::reply_slot_refusals` exists to answer. Reported on
        // the logger rather than on the topic, because the topic is exactly
        // what the reader did not get.
        let dropped = self.state_machine.take_dropped_announcements();
        if dropped > 0 {
            nros_log::log_warn!(
                nros_log::get_logger("nros_lifecycle"),
                "dropped {} lifecycle transition announcement(s): more transitions ran \
                 between two executor spins than TRANSITION_ANNOUNCEMENT_QUEUE holds. \
                 `~/transition_event` has a gap; `~/get_state` is still correct.",
                dropped
            );
        }
    }
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

    /// Issue 1099 — the wire ids and the ENUM DISCRIMINANTS are the same
    /// numbers, so `transition_wire` is identity.
    ///
    /// This is the invariant that was missing. `transition_wire` translated
    /// between two id spaces, so `ros2 lifecycle set` behaved correctly while
    /// the C and C++ APIs — which pass the byte straight through — did not,
    /// about the same node. A translation table that exists is a translation
    /// table that can be the only thing correct.
    ///
    /// Fails on the pre-fix enum at `Cleanup`, `Activate`, `Deactivate` and
    /// `ErrorRecovery`.
    #[test]
    fn transition_ids_match_enum_discriminants() {
        for t in ALL_TRANSITIONS {
            let (wire_id, _) = transition_wire(t);
            assert_eq!(
                wire_id, t as u8,
                "{t:?}: wire id {wire_id} != discriminant {}. The C/C++ APIs pass \
                 the discriminant straight to the state machine, so any gap here is \
                 a transition that means one thing on the wire and another in-process.",
                t as u8
            );
            // ...and the decode is the same function the FFI entry points use.
            assert_eq!(
                from_msg_transition_id(wire_id, transition_start_state(t)),
                Some(t)
            );
            assert_eq!(InternalTransition::from_u8(wire_id), Some(t));
        }
    }

    /// The two upstream ids nano-ros does not implement must decode to
    /// `None` on the wire, not onto some transition we do have (issue 1099 —
    /// `DESTROY` used to be our `ErrorRecovery`).
    #[test]
    fn unimplemented_wire_ids_are_rejected() {
        for state in ALL_STATES {
            assert_eq!(from_msg_transition_id(transition_id::CREATE, state), None);
            assert_eq!(from_msg_transition_id(transition_id::DESTROY, state), None);
        }
    }

    /// The states are NOT identity, and that is deliberate: upstream numbers
    /// `ErrorProcessing` 15 while ours is 5. Unlike the transition ids, `5` is
    /// UNASSIGNED in `lifecycle_msgs/msg/State`, so it can never silently name
    /// a different state — which is why issue 1099 renumbered the transitions
    /// and left the states alone. `state_wire` is the one place that bridges.
    #[test]
    fn state_wire_bridges_the_one_deliberate_divergence() {
        for s in ALL_STATES {
            let (wire_id, _) = state_wire(s);
            if s == InternalState::ErrorProcessing {
                assert_eq!(wire_id, state_id::TRANSITION_STATE_ERRORPROCESSING);
                assert_eq!(wire_id, 15);
                assert_eq!(s as u8, 5);
            } else {
                assert_eq!(wire_id, s as u8, "{s:?} is a primary state; ids match");
            }
        }
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

    // ═══════════════════════════════════════════════════════════════════
    // Phase 86.7 — CDR round-trip tests for generated lifecycle_msgs types
    // ═══════════════════════════════════════════════════════════════════
    //
    // These live here (not in the generated crate) so regenerating
    // `nros-lifecycle-msgs` can't clobber them. They catch codegen drift
    // where the encoder and decoder fall out of sync — any field rename,
    // re-ordering, or missing variant trips the round-trip comparison.

    use nros_core::{CdrReader, Deserialize, Serialize};

    /// Encode `value` to CDR, decode it back, and assert equality.
    fn round_trip<T: Serialize + Deserialize + PartialEq + core::fmt::Debug>(value: T) {
        // Generous scratch for the largest lifecycle message under test.
        const ROUND_TRIP_BUF: usize = 4096;
        let mut buf = [0u8; ROUND_TRIP_BUF];
        let mut writer = crate::tx_writer(&mut buf).expect("writer init");
        value.serialize(&mut writer).expect("serialize");
        let len = writer.position();

        let mut reader = CdrReader::new_with_header(&buf[..len]).expect("reader init");
        let decoded = T::deserialize(&mut reader).expect("deserialize");
        assert_eq!(value, decoded, "CDR round-trip mismatch");
    }

    #[test]
    fn round_trip_state() {
        let mut s = MsgState {
            id: state_id::PRIMARY_STATE_ACTIVE,
            ..Default::default()
        };
        let _ = s.label.push_str("active");
        round_trip(s);
    }

    #[test]
    fn round_trip_state_every_primary() {
        for state in ALL_STATES.iter().copied() {
            round_trip(to_msg_state(state));
        }
    }

    #[test]
    fn round_trip_transition() {
        let mut t = MsgTransition {
            id: transition_id::CONFIGURE,
            ..Default::default()
        };
        let _ = t.label.push_str("configure");
        round_trip(t);
    }

    #[test]
    fn round_trip_transition_every_variant() {
        for trans in ALL_TRANSITIONS.iter().copied() {
            round_trip(to_msg_transition(trans));
        }
    }

    #[test]
    fn round_trip_transition_description() {
        round_trip(build_transition_desc(InternalTransition::Activate));
    }

    #[test]
    fn round_trip_transition_event() {
        use nros_lifecycle_msgs::msg::TransitionEvent;
        let ev = TransitionEvent {
            timestamp: 1_234_567_890,
            transition: to_msg_transition(InternalTransition::Configure),
            start_state: to_msg_state(InternalState::Unconfigured),
            goal_state: to_msg_state(InternalState::Inactive),
        };
        round_trip(ev);
    }

    #[test]
    fn round_trip_change_state_request() {
        let req = ChangeStateRequest {
            transition: to_msg_transition(InternalTransition::Activate),
        };
        round_trip(req);
    }

    #[test]
    fn round_trip_change_state_response() {
        round_trip(ChangeStateResponse { success: true });
        round_trip(ChangeStateResponse { success: false });
    }

    #[test]
    fn round_trip_get_state_request_response() {
        round_trip(GetStateRequest::default());
        let resp = GetStateResponse {
            current_state: to_msg_state(InternalState::Inactive),
        };
        round_trip(resp);
    }

    #[test]
    fn round_trip_get_available_states_request_response() {
        round_trip(GetAvailableStatesRequest::default());
        let mut resp = GetAvailableStatesResponse::default();
        for state in ALL_STATES.iter().copied() {
            let _ = resp.available_states.push(to_msg_state(state));
        }
        round_trip(resp);
    }

    #[test]
    fn round_trip_get_available_transitions_request_response() {
        round_trip(GetAvailableTransitionsRequest::default());
        let mut resp = GetAvailableTransitionsResponse::default();
        for trans in ALL_TRANSITIONS.iter().copied() {
            let _ = resp
                .available_transitions
                .push(build_transition_desc(trans));
        }
        round_trip(resp);
    }

    // ═══════════════════════════════════════════════════════════════════
    // phase-382 W1' — streaming handlers vs. the by-value oracle
    // ═══════════════════════════════════════════════════════════════════
    //
    // The streaming handlers hand-write CDR. The ONLY thing keeping those
    // writes from drifting away from the generated `Serialize` impls is what
    // follows: for each of the five services, serialize a request with the
    // GENERATED impl, run the STREAMING handler over those bytes, then
    //
    //   (a) compare the streamed reply BYTE FOR BYTE against the generated
    //       `Serialize` of whatever the by-value oracle produced for the same
    //       request — this catches encoding drift, and
    //   (b) deserialize the streamed bytes back into the generated reply type
    //       and compare as a VALUE — this catches a reply that encodes to the
    //       right bytes but carries the wrong content.
    //
    // Delete these and the hand-written writes are unguarded.

    use nros_rmw::TransportError;

    /// Scratch for one round trip. Well clear of the largest lifecycle reply
    /// (the full transition graph, a few hundred bytes).
    const WIRE_BUF: usize = 4096;

    /// Encode `value` with the GENERATED `Serialize` and with `hand`, and
    /// assert the two byte strings are identical.
    fn assert_hand_written_matches_generated<T: Serialize + core::fmt::Debug>(
        value: &T,
        hand: impl FnOnce(&mut CdrWriter<'_>) -> Result<(), SerError>,
    ) {
        let mut generated_buf = [0u8; WIRE_BUF];
        let mut hand_buf = [0u8; WIRE_BUF];

        let mut gw = crate::tx_writer(&mut generated_buf).expect("generated writer");
        value.serialize(&mut gw).expect("generated serialize");
        let gen_len = gw.position();

        let mut hw = crate::tx_writer(&mut hand_buf).expect("hand writer");
        hand(&mut hw).expect("hand-written serialize");
        let hand_len = hw.position();

        assert_eq!(
            &generated_buf[..gen_len],
            &hand_buf[..hand_len],
            "hand-written CDR drifted from the generated Serialize for {value:?}"
        );
    }

    /// Serialize `req`, run the streaming `handler` over its wire bytes, and
    /// assert the streamed reply matches `oracle` both as bytes and as a value.
    fn assert_streams_like_oracle<Req, Rep>(
        req: &Req,
        oracle: &Rep,
        handler: impl FnOnce(&mut CdrReader<'_>, &mut CdrWriter<'_>) -> Result<(), TransportError>,
    ) where
        Req: Serialize,
        Rep: Serialize + Deserialize + PartialEq + core::fmt::Debug,
    {
        let mut req_buf = [0u8; WIRE_BUF];
        let mut reply_buf = [0u8; WIRE_BUF];
        let mut oracle_buf = [0u8; WIRE_BUF];

        let mut req_writer = crate::tx_writer(&mut req_buf).expect("request writer");
        req.serialize(&mut req_writer).expect("serialize request");
        let req_len = req_writer.position();

        // Mirrors `handle_request_raw`: the reader sits just past the request's
        // encapsulation header, the writer just past the reply's, and the two
        // buffers are disjoint so both can be live at once.
        let mut reader = CdrReader::new_with_header(&req_buf[..req_len]).expect("request reader");
        let mut writer = crate::tx_writer(&mut reply_buf).expect("reply writer");
        handler(&mut reader, &mut writer).expect("streaming handler");
        let reply_len = writer.position();

        // (a) byte identity against the generated encoder. Both buffers carry
        // the same CDR encapsulation header (`tx_writer`), so this compares
        // the framed reply exactly as `send_response` would put it on the wire.
        let mut oracle_writer = crate::tx_writer(&mut oracle_buf).expect("oracle writer");
        oracle
            .serialize(&mut oracle_writer)
            .expect("generated serialize of the oracle reply");
        let oracle_len = oracle_writer.position();
        assert_eq!(
            &reply_buf[..reply_len],
            &oracle_buf[..oracle_len],
            "streamed reply bytes drifted from the generated Serialize for {oracle:?}"
        );

        // (b) the streamed bytes decode back to the oracle's value.
        let mut reply_reader =
            CdrReader::new_with_header(&reply_buf[..reply_len]).expect("reply reader");
        let decoded = Rep::deserialize(&mut reply_reader).expect("deserialize streamed reply");
        assert_eq!(
            decoded, *oracle,
            "streamed reply decoded to a different value than the by-value oracle"
        );
    }

    unsafe extern "C" fn always_error(_ctx: *mut core::ffi::c_void) -> u8 {
        nros_core::lifecycle::TransitionResult::Error as u8
    }

    /// A state machine sitting in `state`, with no callbacks left registered
    /// so both the streaming and by-value paths see identical behaviour.
    fn sm_at(state: InternalState) -> LifecyclePollingNodeCtx {
        let mut sm = LifecyclePollingNodeCtx::new();
        // SAFETY: the only callback ever registered here is `always_error`, a
        // 'static `extern "C"` fn that ignores its (null) context pointer.
        unsafe {
            match state {
                InternalState::Unconfigured => {}
                InternalState::Inactive => {
                    sm.trigger_transition(InternalTransition::Configure)
                        .expect("configure");
                }
                InternalState::Active => {
                    sm.trigger_transition(InternalTransition::Configure)
                        .expect("configure");
                    sm.trigger_transition(InternalTransition::Activate)
                        .expect("activate");
                }
                InternalState::Finalized => {
                    sm.trigger_transition(InternalTransition::ShutdownUnconfigured)
                        .expect("shutdown");
                }
                InternalState::ErrorProcessing => {
                    sm.register(
                        crate::lifecycle::LifecycleCallbackSlot::Configure,
                        Some(always_error),
                    );
                    let r = sm.trigger_transition(InternalTransition::Configure);
                    assert!(r.is_err(), "an erroring callback must fail the transition");
                    sm.clear_callbacks();
                }
            }
        }
        assert_eq!(
            sm.state(),
            state,
            "sm_at failed to reach the requested state"
        );
        sm
    }

    fn change_state_request(id: u8, label: &str) -> ChangeStateRequest {
        let mut req = ChangeStateRequest::default();
        req.transition.id = id;
        req.transition
            .label
            .push_str(label)
            .expect("test label fits the bounded string");
        req
    }

    /// Run one `ChangeState` request down both paths, against two state
    /// machines started in the same state, and assert the replies agree AND
    /// that the state machines ended up in the same place — `change_state` is
    /// the one handler with a side effect, so the reply alone is not enough.
    fn assert_change_state_matches_oracle(start: InternalState, id: u8, label: &str) {
        let req = change_state_request(id, label);

        let mut oracle_sm = sm_at(start);
        // SAFETY: `sm_at` clears every callback, so no fn pointer is invoked.
        let oracle = unsafe { handle_change_state(&mut oracle_sm, &req) };

        let mut streamed_sm = sm_at(start);
        assert_streams_like_oracle(&req, &*oracle, |r, w| {
            // SAFETY: as above.
            unsafe { stream_change_state(&mut streamed_sm, r, w) }
        });

        assert_eq!(
            streamed_sm.state(),
            oracle_sm.state(),
            "streaming change_state left the state machine somewhere else"
        );
    }

    #[test]
    fn wire_writers_match_generated_serialize() {
        for state in ALL_STATES {
            assert_hand_written_matches_generated(&to_msg_state(state), |w| write_state(w, state));
        }
        for t in ALL_TRANSITIONS {
            assert_hand_written_matches_generated(&to_msg_transition(t), |w| {
                write_transition(w, t)
            });
            assert_hand_written_matches_generated(&build_transition_desc(t), |w| {
                write_transition_desc(w, t)
            });
        }
    }

    /// Issue 1587 — the hand-written `TransitionEvent` writer, over every
    /// (transition, start, goal) triple the state machine can actually
    /// produce, against the generated `Serialize`.
    #[test]
    fn transition_event_writer_matches_generated_serialize() {
        use nros_lifecycle_msgs::msg::TransitionEvent;

        for t in ALL_TRANSITIONS {
            for start in ALL_STATES {
                for goal in ALL_STATES {
                    let value = TransitionEvent {
                        timestamp: 0x0123_4567_89ab_cdef,
                        transition: to_msg_transition(t),
                        start_state: to_msg_state(start),
                        goal_state: to_msg_state(goal),
                    };
                    assert_hand_written_matches_generated(&value, |w| {
                        write_transition_event(w, value.timestamp, t, start, goal)
                    });
                }
            }
        }
    }

    /// The buffer is DERIVED arithmetic, so measure the widest thing that can
    /// go in it rather than trusting the sum.
    #[test]
    fn a_maximal_transition_event_fits_the_derived_buffer() {
        let mut worst = 0usize;
        for t in ALL_TRANSITIONS {
            for start in ALL_STATES {
                for goal in ALL_STATES {
                    let mut buf = [0u8; WIRE_BUF];
                    let mut w = crate::tx_writer(&mut buf).expect("writer");
                    write_transition_event(&mut w, u64::MAX, t, start, goal).expect("encode");
                    worst = worst.max(w.position());
                }
            }
        }
        assert!(
            worst <= TRANSITION_EVENT_TX_BUF,
            "a maximal TransitionEvent encodes to {worst} bytes, over the derived \
             TRANSITION_EVENT_TX_BUF of {TRANSITION_EVENT_TX_BUF}"
        );
    }

    /// Issue 1587 — the measurement the whole price of this row turned on.
    ///
    /// phase-467's study assumed `~/transition_event` was TRANSIENT_LOCAL and
    /// priced it as the most expensive of its thirteen rows on that basis: a
    /// transient-local publisher also declares a zenoh cache queryable (issue
    /// 1378), and `ZPICO_MAX_QUERYABLES` defaults to 8 embedded against the
    /// eleven `[param_services]` + `[lifecycle]` already claim (issue 0460).
    ///
    /// MEASURED instead, twice, against a live
    /// `ros2 run lifecycle lifecycle_talker` on humble:
    /// `ros2 topic info -v /lc_talker/transition_event` reports RELIABLE /
    /// VOLATILE / Infinite lifespan. That is `rmw_qos_profile_default`, which
    /// `rcl_lifecycle`'s `com_interface.c` takes from
    /// `rcl_publisher_get_default_options()` and never overrides. So this
    /// publisher costs ZERO queryables, and the expensive half of the row
    /// does not exist.
    ///
    /// A guard, not a restatement: a future "let's make it transient-local so
    /// a late supervisor sees the last transition" would move the queryable
    /// floor of every lifecycle image, and would fail here first.
    #[test]
    fn the_transition_event_publisher_is_volatile() {
        use nros_rmw::{QoSDurabilityPolicy, QoSHistoryPolicy, QoSReliabilityPolicy};
        let qos = transition_event_qos();
        assert_eq!(
            qos.durability,
            QoSDurabilityPolicy::Volatile,
            "upstream's ~/transition_event is VOLATILE; transient-local would cost \
             every lifecycle image a zenoh cache queryable (issues 1378 / 0460)"
        );
        assert_eq!(qos.reliability, QoSReliabilityPolicy::Reliable);
        assert_eq!(qos.history, QoSHistoryPolicy::KeepLast);
        assert_eq!(qos.depth, 10, "rmw_qos_profile_default's depth");
    }

    #[test]
    fn empty_label_encodes_like_the_generated_string_writer() {
        // No lifecycle state or transition carries an empty label today, but
        // the hand-written `write_string` path must still agree with the
        // generated one for the degenerate case — a length-1 CDR string
        // holding only its null terminator — or a default-constructed State
        // would encode differently on the two paths.
        let msg = MsgState::default();
        assert!(msg.label.is_empty(), "State::default has an empty label");
        assert_hand_written_matches_generated(&msg, |w| {
            write_id_label(w, msg.id, msg.label.as_str())
        });

        let t = MsgTransition::default();
        assert!(t.label.is_empty(), "Transition::default has an empty label");
        assert_hand_written_matches_generated(&t, |w| write_id_label(w, t.id, t.label.as_str()));
    }

    #[test]
    fn change_state_streams_like_oracle_when_accepted() {
        assert_change_state_matches_oracle(
            InternalState::Unconfigured,
            transition_id::CONFIGURE,
            "configure",
        );
        assert_change_state_matches_oracle(
            InternalState::Active,
            transition_id::DEACTIVATE,
            "deactivate",
        );
    }

    #[test]
    fn change_state_streams_like_oracle_when_rejected() {
        // ACTIVATE is a known id but is not reachable from Unconfigured, so
        // `trigger_transition` returns InvalidTransition and success is false.
        assert_change_state_matches_oracle(
            InternalState::Unconfigured,
            transition_id::ACTIVATE,
            "activate",
        );
        // Finalized is terminal — every transition is refused from there.
        assert_change_state_matches_oracle(
            InternalState::Finalized,
            transition_id::CLEANUP,
            "cleanup",
        );
    }

    #[test]
    fn change_state_streams_like_oracle_for_unknown_transition_id() {
        // 99 maps to no internal transition, and the label is EMPTY — a
        // length-1 CDR string carrying only its null terminator, which the
        // streaming reader must accept rather than read as truncation.
        assert_change_state_matches_oracle(InternalState::Inactive, 99, "");
        // DESTROY is a real lifecycle_msgs id that this implementation does
        // not support; it must be refused, not silently mapped.
        assert_change_state_matches_oracle(
            InternalState::Inactive,
            transition_id::DESTROY,
            "destroy",
        );
    }

    #[test]
    fn change_state_streams_like_oracle_when_resolved_by_label() {
        // id 0 (CREATE) maps to no internal transition, so the LABEL decides —
        // and "shutdown" resolves to a different variant per state.
        for state in [
            InternalState::Unconfigured,
            InternalState::Inactive,
            InternalState::Active,
        ] {
            assert_change_state_matches_oracle(state, transition_id::CREATE, "shutdown");
        }
    }

    #[test]
    fn get_state_streams_like_oracle_in_every_state() {
        for state in ALL_STATES {
            let sm = sm_at(state);
            let req = GetStateRequest::default();
            let oracle = handle_get_state(&sm, &req);
            assert_streams_like_oracle(&req, &*oracle, |r, w| stream_get_state(&sm, r, w));
        }
    }

    #[test]
    fn get_available_states_streams_like_oracle() {
        for state in ALL_STATES {
            let sm = sm_at(state);
            let req = GetAvailableStatesRequest::default();
            let oracle = handle_get_available_states(&sm, &req);
            assert_eq!(
                oracle.available_states.len(),
                ALL_STATES.len(),
                "the state list is state-independent"
            );
            assert_streams_like_oracle(&req, &*oracle, |r, w| {
                stream_get_available_states(&sm, r, w)
            });
        }
    }

    #[test]
    fn get_available_transitions_streams_like_oracle_in_every_state() {
        // Includes Finalized, whose answer is the EMPTY sequence — the
        // `write_sequence_len(0)` case.
        let mut saw_empty = false;
        for state in ALL_STATES {
            let sm = sm_at(state);
            let req = GetAvailableTransitionsRequest::default();
            let oracle = handle_get_available_transitions(&sm, &req);
            saw_empty |= oracle.available_transitions.is_empty();
            assert_streams_like_oracle(&req, &*oracle, |r, w| {
                stream_get_available_transitions(&sm, r, w)
            });
        }
        assert!(
            saw_empty,
            "no state produced an empty transition list — the zero-length \
             sequence encoding went untested"
        );
    }

    #[test]
    fn get_transition_graph_streams_the_whole_table_like_oracle() {
        for state in ALL_STATES {
            let sm = sm_at(state);
            let req = GetAvailableTransitionsRequest::default();
            let oracle = handle_get_transition_graph(&sm, &req);
            assert_eq!(
                oracle.available_transitions.len(),
                ALL_TRANSITIONS.len(),
                "the graph is state-independent: every ALL_TRANSITIONS entry appears"
            );
            assert_streams_like_oracle(&req, &*oracle, |r, w| {
                stream_get_transition_graph(&sm, r, w)
            });
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    // Phase 86.8 — Integration tests via MockSession
    // ═══════════════════════════════════════════════════════════════════
    //
    // These exercise the full `Executor::register_lifecycle_services`
    // wiring: creating the five service-server handles, mounting the
    // state machine on the executor, and draining services during
    // spin_once. With `MockSession` every service server returns
    // `Ok(None)` from `take_request`, so the tests confirm the
    // plumbing doesn't crash when there's nothing to process and that
    // the state machine accessors behave correctly.
    //
    // Wrapped in a sub-module gated on the same condition as
    // `MockSession`'s `ConcreteSession` alias in `session.rs` —
    // feature unification under `cargo test --workspace` activates
    // one of the rmw-* features through downstream crates (e.g.
    // nros-px4), at which point `Executor::from_session` expects a
    // different concrete session and these tests stop type-checking.
    #[cfg(not(feature = "rmw-cffi"))]
    mod mock_integration {
        use super::*;
        use crate::{executor::Executor, mock::MockSession};
        use core::{
            ffi::c_void,
            sync::atomic::{AtomicU32, Ordering},
            time::Duration,
        };
        use nros_core::lifecycle::TransitionResult;

        #[test]
        fn register_lifecycle_services_succeeds_on_mock() {
            let session = MockSession::new();
            let mut executor: Executor = Executor::from_session(session);
            executor
                .register_lifecycle_services()
                .expect("register on MockSession should succeed");
            assert!(
                executor.lifecycle_state_machine().is_some(),
                "state machine should exist after registration"
            );
            assert_eq!(
                executor.lifecycle_state_machine().unwrap().state(),
                InternalState::Unconfigured,
                "fresh state machine starts in Unconfigured"
            );
        }

        #[test]
        fn state_machine_absent_before_registration() {
            let session = MockSession::new();
            let executor: Executor = Executor::from_session(session);
            assert!(executor.lifecycle_state_machine().is_none());
        }

        // ───────────────────────────────────────────────────────────────
        // Issue 1587 — the `~/transition_event` publisher
        // ───────────────────────────────────────────────────────────────

        /// Decode what the mock publisher was last handed back into the
        /// GENERATED type, so these assertions are about the bytes a
        /// subscriber receives and not about our own writer's intent.
        fn last_event(
            executor: &Executor,
        ) -> Option<nros_lifecycle_msgs::msg::TransitionEvent> {
            use nros_core::Deserialize;
            let pubr = executor.lifecycle_transition_event_publisher()?;
            let (bytes, len) = pubr.last_published()?;
            // Skip the 4-byte CDR encapsulation header the writer prepends.
            let mut reader = CdrReader::new(&bytes[4..len]);
            Some(
                nros_lifecycle_msgs::msg::TransitionEvent::deserialize(&mut reader)
                    .expect("the published bytes decode as a TransitionEvent"),
            )
        }

        #[test]
        fn the_transition_event_publisher_uses_the_rep_2002_topic_name() {
            let session = MockSession::new();
            let mut executor: Executor = Executor::from_session(session);
            executor.register_lifecycle_services().unwrap();
            let topic = executor
                .lifecycle_transition_event_publisher()
                .expect("registration creates the publisher")
                .topic_name();
            assert!(
                topic.ends_with("/transition_event"),
                "`~/transition_event` resolves to <node fqn>/transition_event; got {topic:?}"
            );
        }

        #[test]
        fn a_transition_is_announced_on_transition_event() {
            let session = MockSession::new();
            let mut executor: Executor = Executor::from_session(session);
            executor.register_lifecycle_services().unwrap();

            // Nothing has transitioned, so nothing has been announced.
            executor.spin_once(Duration::from_millis(0));
            assert_eq!(
                executor
                    .lifecycle_transition_event_publisher()
                    .unwrap()
                    .publish_count(),
                0,
                "a spin with no transition announces nothing"
            );

            // SAFETY: no callback is registered, so no FFI pointer is called.
            let sm = executor.lifecycle_state_machine_mut().unwrap();
            unsafe { sm.trigger_transition(InternalTransition::Configure) }.expect("configure");
            executor.spin_once(Duration::from_millis(0));

            assert_eq!(
                executor
                    .lifecycle_transition_event_publisher()
                    .unwrap()
                    .publish_count(),
                1,
                "one transition, one announcement"
            );
            let ev = last_event(&executor).expect("an event reached the wire");
            assert_eq!(ev.transition.id, transition_id::CONFIGURE);
            assert_eq!(ev.transition.label.as_str(), "configure");
            assert_eq!(ev.start_state.id, state_id::PRIMARY_STATE_UNCONFIGURED);
            assert_eq!(ev.goal_state.id, state_id::PRIMARY_STATE_INACTIVE);
        }

        #[test]
        fn a_burst_before_the_first_spin_is_announced_in_order() {
            // `nros_cpp_lifecycle_autostart(exec, 2)` runs configure then
            // activate from `__nros_entry_setup`, before the spin loop
            // exists. Both must reach the wire, in order, at the first spin —
            // that is what TRANSITION_ANNOUNCEMENT_QUEUE is sized for.
            let session = MockSession::new();
            let mut executor: Executor = Executor::from_session(session);
            executor.register_lifecycle_services().unwrap();

            let sm = executor.lifecycle_state_machine_mut().unwrap();
            // SAFETY: no callbacks registered.
            unsafe {
                sm.trigger_transition(InternalTransition::Configure).unwrap();
                sm.trigger_transition(InternalTransition::Activate).unwrap();
            }
            executor.spin_once(Duration::from_millis(0));

            assert_eq!(
                executor
                    .lifecycle_transition_event_publisher()
                    .unwrap()
                    .publish_count(),
                2,
                "both autostart transitions are announced"
            );
            let ev = last_event(&executor).expect("an event reached the wire");
            assert_eq!(
                ev.transition.id,
                transition_id::ACTIVATE,
                "the LAST announcement is the LAST transition, so the order is FIFO"
            );
            assert_eq!(ev.goal_state.id, state_id::PRIMARY_STATE_ACTIVE);
        }

        #[test]
        fn a_failed_transition_is_announced_too() {
            // rcl publishes the failure transition as well, and a supervisor
            // watching passively is exactly the reader who needs to see a
            // `Configure` that landed somewhere other than `Inactive`.
            unsafe extern "C" fn fail(_ctx: *mut c_void) -> u8 {
                TransitionResult::Failure as u8
            }
            let session = MockSession::new();
            let mut executor: Executor = Executor::from_session(session);
            executor.register_lifecycle_services().unwrap();
            let sm = executor.lifecycle_state_machine_mut().unwrap();
            sm.register(
                crate::lifecycle::LifecycleCallbackSlot::Configure,
                Some(fail),
            );
            // SAFETY: `fail` touches neither the context pointer nor anything
            // outside this frame.
            let _ = unsafe { sm.trigger_transition(InternalTransition::Configure) };
            executor.spin_once(Duration::from_millis(0));

            let ev = last_event(&executor).expect("a failed transition still announces");
            assert_eq!(ev.transition.id, transition_id::CONFIGURE);
            assert_ne!(
                ev.goal_state.id,
                state_id::PRIMARY_STATE_INACTIVE,
                "the announcement carries where the machine ACTUALLY landed"
            );
        }

        #[test]
        fn a_rejected_transition_announces_nothing() {
            // `Activate` from `Unconfigured` never runs, so there is nothing
            // to announce — the two early returns in `trigger_transition` sit
            // before the record, as they do in rcl.
            let session = MockSession::new();
            let mut executor: Executor = Executor::from_session(session);
            executor.register_lifecycle_services().unwrap();
            let sm = executor.lifecycle_state_machine_mut().unwrap();
            // SAFETY: no callbacks registered.
            assert!(unsafe { sm.trigger_transition(InternalTransition::Activate) }.is_err());
            executor.spin_once(Duration::from_millis(0));
            assert_eq!(
                executor
                    .lifecycle_transition_event_publisher()
                    .unwrap()
                    .publish_count(),
                0
            );
        }

        #[test]
        fn spin_once_drains_empty_lifecycle_services_cleanly() {
            let session = MockSession::new();
            let mut executor: Executor = Executor::from_session(session);
            executor.register_lifecycle_services().unwrap();

            // No requests are queued on MockServiceServer, so spin_once must
            // return without incrementing services_handled and without panic.
            let result = executor.spin_once(Duration::from_millis(0));
            assert_eq!(result.services_handled, 0);
            assert_eq!(result.service_errors, 0);
            assert!(!result.any_work());
        }

        static CB_CALLS: AtomicU32 = AtomicU32::new(0);

        unsafe extern "C" fn record_success(_ctx: *mut c_void) -> u8 {
            CB_CALLS.fetch_add(1, Ordering::SeqCst);
            TransitionResult::Success as u8
        }

        #[test]
        fn executor_accessor_drives_full_state_machine_cycle() {
            let session = MockSession::new();
            let mut executor: Executor = Executor::from_session(session);
            executor.register_lifecycle_services().unwrap();

            CB_CALLS.store(0, Ordering::SeqCst);

            // Register callbacks through the executor accessor and walk the
            // happy path: Unconfigured → Inactive → Active → Inactive → Unconfigured.
            let sm = executor.lifecycle_state_machine_mut().unwrap();
            sm.register(
                crate::lifecycle::LifecycleCallbackSlot::Configure,
                Some(record_success),
            );
            sm.register(
                crate::lifecycle::LifecycleCallbackSlot::Activate,
                Some(record_success),
            );
            sm.register(
                crate::lifecycle::LifecycleCallbackSlot::Deactivate,
                Some(record_success),
            );
            sm.register(
                crate::lifecycle::LifecycleCallbackSlot::Cleanup,
                Some(record_success),
            );

            // SAFETY: callbacks have 'static lifetime; ctx is null (unused).
            unsafe {
                sm.trigger_transition(InternalTransition::Configure)
                    .unwrap();
                assert_eq!(sm.state(), InternalState::Inactive);
                sm.trigger_transition(InternalTransition::Activate).unwrap();
                assert_eq!(sm.state(), InternalState::Active);
                sm.trigger_transition(InternalTransition::Deactivate)
                    .unwrap();
                assert_eq!(sm.state(), InternalState::Inactive);
                sm.trigger_transition(InternalTransition::Cleanup).unwrap();
                assert_eq!(sm.state(), InternalState::Unconfigured);
            }
            assert_eq!(CB_CALLS.load(Ordering::SeqCst), 4);
        }

        #[test]
        fn change_state_handler_via_executor_accessor() {
            let session = MockSession::new();
            let mut executor: Executor = Executor::from_session(session);
            executor.register_lifecycle_services().unwrap();

            // Drive `handle_change_state` directly against the executor's
            // state machine, simulating what the service dispatcher does
            // when a `ChangeState` request arrives.
            let sm = executor.lifecycle_state_machine_mut().unwrap();
            let mut req = ChangeStateRequest::default();
            req.transition.id = transition_id::CONFIGURE;

            // SAFETY: no callback registered; trigger_transition uses the
            // implicit Success result.
            let resp = unsafe { handle_change_state(sm, &req) };
            assert!(resp.success);
            assert_eq!(sm.state(), InternalState::Inactive);

            // Subsequent `get_state` handler must reflect the new state.
            let gs = handle_get_state(sm, &GetStateRequest::default());
            assert_eq!(gs.current_state.id, state_id::PRIMARY_STATE_INACTIVE);
        }
    }
}
