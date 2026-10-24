//! Wire framing of the action messages an action CLIENT takes off the wire —
//! the ONE place their payload offsets are spelled (issue 2014).
//!
//! The producer side (`action_core::write_get_result_header`,
//! `publish_feedback_raw`) writes these frames with a [`CdrWriter`], so its
//! offsets fall out of `align()` and are never written down. Every CONSUMER
//! used to spell them itself — the arena dispatch, the typed dispatch, the C
//! and C++ polling paths, the C++ `PollingActionClient` header — and they
//! disagreed: the header skipped 5 bytes of a get_result reply whose payload
//! starts at 8, and the C poll path handed feedback fields to a callback with
//! no encapsulation header while the arena handed it one.
//!
//! Frames (all little-endian CDR, top-level encapsulation header first):
//!
//! * `SendGoal` reply: `[encap 4][accepted u8][stamp …]`
//! * `GetResult` reply: `[encap 4][status i8][pad to 4 → byte 8][result fields]`
//! * `FeedbackMessage`: `[encap 4][GoalId.uuid 16][feedback fields]`
//! * `SendGoal` request (server side): `[encap 4][GoalId.uuid 16][goal fields]`
//! * `CancelGoal` reply: `[encap 4][return_code i8][pad to 4 → byte 8][count u32]
//!   [GoalInfo { uuid 16, stamp.sec i32, stamp.nanosec u32 } × count]`
//!
//! The payload fields arrive WITHOUT an encapsulation header of their own on
//! every backend (RFC-0069 / issue 0418), and every deserializer reads with
//! `new_with_header`, so a consumer [`splice`]s the frame's own top-level
//! header in front of them. That is the shape every callback receives.
//!
//! [`CdrWriter`]: nros_core::CdrWriter
//! [`splice`]: crate::action_frame::splice

use nros_core::{GoalId, GoalStatus};

/// Length of a CDR encapsulation header.
pub const CDR_HEADER_LEN: usize = nros_serdes::CDR_HEADER_LEN;

/// Byte holding the `accepted` flag of a `SendGoal` reply and the `status` of
/// a `GetResult` reply — the first field after the encapsulation header.
pub const REPLY_STATUS_OFFSET: usize = CDR_HEADER_LEN;

/// First byte of the result fields in a `GetResult` reply: the `i8` status,
/// then CDR alignment to 4 (stream position 4 = byte 8).
pub const RESULT_PAYLOAD_OFFSET: usize = (REPLY_STATUS_OFFSET + 1 + 3) & !3;

/// First byte of the feedback fields in a `FeedbackMessage`: the GoalId UUID
/// (`uint8[16]`, no length prefix since 233.6) follows the header.
pub const FEEDBACK_PAYLOAD_OFFSET: usize =
    CDR_HEADER_LEN + GoalId::SEQ_PREFIX_LEN + GoalId::UUID_LEN;

/// First byte of the goal fields in a `SendGoal` request — the same framing
/// as feedback: the GoalId follows the header.
pub const GOAL_REQUEST_PAYLOAD_OFFSET: usize = FEEDBACK_PAYLOAD_OFFSET;

/// Byte holding the `goals_canceling` sequence length of a `CancelGoal`
/// reply: the `i8` return code, then CDR alignment to 4 (issue 2027).
pub const CANCEL_GOALS_COUNT_OFFSET: usize = (REPLY_STATUS_OFFSET + 1 + 3) & !3;

/// First `GoalInfo` of a `CancelGoal` reply's `goals_canceling`.
pub const CANCEL_GOALS_OFFSET: usize = CANCEL_GOALS_COUNT_OFFSET + 4;

/// One `action_msgs/msg/GoalInfo` on the wire: the UUID (`uint8[16]`), then
/// `builtin_interfaces/Time` (`int32 sec`, `uint32 nanosec`, 4-aligned, which
/// 16 bytes in already is).
pub const GOAL_INFO_LEN: usize = GoalId::UUID_LEN + 4 + 4;

const _: () = assert!(RESULT_PAYLOAD_OFFSET == 8);
const _: () = assert!(CANCEL_GOALS_OFFSET == 12);
const _: () = assert!(FEEDBACK_PAYLOAD_OFFSET == 20);

/// The `accepted` flag of a `SendGoal` reply (`false` for a short frame).
pub fn goal_accepted(reply: &[u8]) -> bool {
    reply.len() > REPLY_STATUS_OFFSET && reply[REPLY_STATUS_OFFSET] != 0
}

/// The goal UUID a `FeedbackMessage` names (`None` for a short frame).
pub fn feedback_goal_id(msg: &[u8]) -> Option<GoalId> {
    let bytes = msg.get(CDR_HEADER_LEN..FEEDBACK_PAYLOAD_OFFSET)?;
    let mut uuid = [0u8; GoalId::UUID_LEN];
    uuid.copy_from_slice(bytes);
    Some(GoalId { uuid })
}

/// The terminal status of a `GetResult` reply, or `None` when the frame is
/// too short to hold a result (shorter than [`RESULT_PAYLOAD_OFFSET`]).
/// Anything but Succeeded / Canceled / Aborted reads as `Unknown`.
pub fn result_status(reply: &[u8]) -> Option<GoalStatus> {
    if reply.len() < RESULT_PAYLOAD_OFFSET {
        return None;
    }
    Some(match reply[REPLY_STATUS_OFFSET] {
        4 => GoalStatus::Succeeded,
        5 => GoalStatus::Canceled,
        6 => GoalStatus::Aborted,
        _ => GoalStatus::Unknown,
    })
}

/// The result fields of a `GetResult` reply, without any header (`None` when
/// the frame is shorter than [`RESULT_PAYLOAD_OFFSET`]).
pub fn result_fields(reply: &[u8]) -> Option<&[u8]> {
    reply.get(RESULT_PAYLOAD_OFFSET..)
}

/// The feedback fields of a `FeedbackMessage`, without any header. `None`
/// when the frame carries no fields at all — an empty feedback is not
/// delivered, which is what every consumer did before this helper existed.
pub fn feedback_fields(msg: &[u8]) -> Option<&[u8]> {
    if msg.len() <= FEEDBACK_PAYLOAD_OFFSET {
        return None;
    }
    msg.get(FEEDBACK_PAYLOAD_OFFSET..)
}

/// The goal fields of a `SendGoal` request, without any header (`None` when
/// the frame carries no fields).
pub fn goal_request_fields(req: &[u8]) -> Option<&[u8]> {
    if req.len() <= GOAL_REQUEST_PAYLOAD_OFFSET {
        return None;
    }
    req.get(GOAL_REQUEST_PAYLOAD_OFFSET..)
}

/// The `return_code` of a `CancelGoal` reply (`None` for a short frame).
pub fn cancel_return_code(reply: &[u8]) -> Option<i8> {
    reply.get(REPLY_STATUS_OFFSET).map(|b| *b as i8)
}

/// How many goals a `CancelGoal` reply says are canceling — issue 2027.
/// `None` when the frame is too short to hold the count, or the count names
/// more `GoalInfo`s than the frame carries (a truncation is not "none").
pub fn cancel_goals_count(reply: &[u8]) -> Option<usize> {
    let raw = reply.get(CANCEL_GOALS_COUNT_OFFSET..CANCEL_GOALS_OFFSET)?;
    let n = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
    let need = n
        .checked_mul(GOAL_INFO_LEN)?
        .checked_add(CANCEL_GOALS_OFFSET)?;
    (reply.len() >= need).then_some(n)
}

/// The `i`-th `GoalInfo` of a `CancelGoal` reply's `goals_canceling`: the
/// goal and its acceptance stamp `(sec, nanosec)`. `None` past the count (or
/// on a short frame).
pub fn cancel_goal_info(reply: &[u8], i: usize) -> Option<(GoalId, i32, u32)> {
    if i >= cancel_goals_count(reply)? {
        return None;
    }
    let at = CANCEL_GOALS_OFFSET + i * GOAL_INFO_LEN;
    let b = reply.get(at..at + GOAL_INFO_LEN)?;
    let mut uuid = [0u8; GoalId::UUID_LEN];
    uuid.copy_from_slice(&b[..GoalId::UUID_LEN]);
    let s = GoalId::UUID_LEN;
    let sec = i32::from_le_bytes([b[s], b[s + 1], b[s + 2], b[s + 3]]);
    let nanosec = u32::from_le_bytes([b[s + 4], b[s + 5], b[s + 6], b[s + 7]]);
    Some((GoalId { uuid }, sec, nanosec))
}

/// Write `frame`'s own encapsulation header followed by `fields` into `out`,
/// truncating `fields` to what fits. Returns the bytes written, or `None` when
/// `frame` has no header or `out` cannot hold one.
pub fn splice(frame: &[u8], fields: &[u8], out: &mut [u8]) -> Option<usize> {
    if frame.len() < CDR_HEADER_LEN || out.len() < CDR_HEADER_LEN {
        return None;
    }
    let n = fields.len().min(out.len() - CDR_HEADER_LEN);
    out[..CDR_HEADER_LEN].copy_from_slice(&frame[..CDR_HEADER_LEN]);
    out[CDR_HEADER_LEN..CDR_HEADER_LEN + n].copy_from_slice(&fields[..n]);
    Some(CDR_HEADER_LEN + n)
}

/// A `GetResult` reply's result, header-framed for a deserializer.
pub fn splice_result(reply: &[u8], out: &mut [u8]) -> Option<usize> {
    splice(reply, result_fields(reply)?, out)
}

/// A `FeedbackMessage`'s feedback, header-framed for a deserializer.
pub fn splice_feedback(msg: &[u8], out: &mut [u8]) -> Option<usize> {
    splice(msg, feedback_fields(msg)?, out)
}

/// A `SendGoal` request's goal, header-framed for a deserializer.
pub fn splice_goal_request(req: &[u8], out: &mut [u8]) -> Option<usize> {
    splice(req, goal_request_fields(req)?, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_payload_starts_after_the_alignment_pad() {
        // [encap][status=4][pad pad pad][fields 0xAA 0xBB]
        let reply = [0, 1, 0, 0, 4, 0xEE, 0xEE, 0xEE, 0xAA, 0xBB];
        assert_eq!(result_status(&reply), Some(GoalStatus::Succeeded));
        assert_eq!(result_fields(&reply), Some(&[0xAA, 0xBB][..]));
        let mut out = [0u8; 16];
        let n = splice_result(&reply, &mut out).unwrap();
        assert_eq!(&out[..n], &[0, 1, 0, 0, 0xAA, 0xBB]);
    }

    #[test]
    fn cancel_reply_lists_its_canceling_goals() {
        // [encap][code=0][pad][count=2][uuid 16, sec, nsec] x 2
        let mut reply = [0u8; CANCEL_GOALS_OFFSET + 2 * GOAL_INFO_LEN];
        reply[..4].copy_from_slice(&[0, 1, 0, 0]);
        reply[5..8].copy_from_slice(&[0xEE; 3]);
        reply[8..12].copy_from_slice(&2u32.to_le_bytes());
        reply[12] = 9;
        reply[28..32].copy_from_slice(&5i32.to_le_bytes());
        reply[32..36].copy_from_slice(&7u32.to_le_bytes());
        reply[36] = 10;
        assert_eq!(cancel_return_code(&reply), Some(0));
        assert_eq!(cancel_goals_count(&reply), Some(2));
        let (g, sec, ns) = cancel_goal_info(&reply, 0).unwrap();
        assert_eq!((g.uuid[0], sec, ns), (9, 5, 7));
        assert_eq!(cancel_goal_info(&reply, 1).unwrap().0.uuid[0], 10);
        assert!(cancel_goal_info(&reply, 2).is_none());
        // A count naming more entries than the frame holds is a truncation.
        assert_eq!(cancel_goals_count(&reply[..CANCEL_GOALS_OFFSET + 4]), None);
        assert_eq!(cancel_goals_count(&[0, 1, 0, 0, 0]), None);
    }

    #[test]
    fn short_frames_carry_nothing() {
        assert_eq!(result_status(&[0, 1, 0, 0, 4]), None);
        assert_eq!(feedback_fields(&[0u8; FEEDBACK_PAYLOAD_OFFSET]), None);
        assert!(!goal_accepted(&[0, 1, 0, 0]));
    }

    #[test]
    fn feedback_splices_the_frames_own_header() {
        let mut msg = [0u8; FEEDBACK_PAYLOAD_OFFSET + 2];
        msg[..4].copy_from_slice(&[0, 0, 0, 0]); // big-endian encap
        msg[4] = 7; // first uuid byte
        msg[FEEDBACK_PAYLOAD_OFFSET..].copy_from_slice(&[1, 2]);
        assert_eq!(feedback_goal_id(&msg).unwrap().uuid[0], 7);
        let mut out = [0u8; 8];
        let n = splice_feedback(&msg, &mut out).unwrap();
        assert_eq!(&out[..n], &[0, 0, 0, 0, 1, 2]);
    }
}
