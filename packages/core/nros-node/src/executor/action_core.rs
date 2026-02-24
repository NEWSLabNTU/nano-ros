//! Type-agnostic action protocol core types.
//!
//! [`ActionServerCore`] and [`ActionClientCore`] handle the raw-bytes
//! action protocol (GoalId framing, status publishing, result slab)
//! without requiring `RosAction` type parameters. The typed
//! [`ActionServer`](super::handles::ActionServer) and
//! [`ActionClient`](super::handles::ActionClient) wrap these cores
//! and add serialization/deserialization at the boundary.

use nros_core::{CdrReader, CdrWriter, GoalId, GoalInfo, GoalStatus, GoalStatusStamped, Serialize};
use nros_rmw::{Publisher, ServiceServerTrait, Subscriber, TransportError};

use super::types::NodeError;

// ============================================================================
// Supporting types
// ============================================================================

/// Goal tracked by the core — only GoalId + status, no typed data.
#[derive(Clone, Copy)]
pub struct RawActiveGoal {
    /// Goal ID.
    pub goal_id: GoalId,
    /// Current status.
    pub status: GoalStatus,
}

/// Completed goal result metadata — indexes into the result slab.
#[derive(Clone, Copy)]
pub struct CompletedResultEntry {
    /// Unique identifier for the completed goal.
    pub goal_id: GoalId,
    /// Terminal status of the goal.
    pub status: GoalStatus,
    /// Byte offset into the result slab.
    pub offset: usize,
    /// Length of the serialised result in bytes.
    pub len: usize,
}

/// Information about a received goal request.
pub struct RawGoalRequest {
    /// The parsed goal ID.
    pub goal_id: GoalId,
    /// Sequence number for the service reply.
    pub sequence_number: i64,
    /// Total length of valid CDR data in the goal buffer.
    pub data_len: usize,
}

// ============================================================================
// GoalId CDR helpers
// ============================================================================

/// Read a GoalId from a CDR reader (4-byte sequence length + 16 UUID bytes).
fn read_goal_id(reader: &mut CdrReader<'_>) -> Result<GoalId, NodeError> {
    let uuid_len = reader
        .read_u32()
        .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?
        as usize;
    let mut goal_id = GoalId::default();
    if uuid_len == 16 {
        for byte in &mut goal_id.uuid {
            *byte = reader
                .read_u8()
                .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;
        }
    }
    Ok(goal_id)
}

/// Write a GoalId into a CDR writer (4-byte sequence length + 16 UUID bytes).
fn write_goal_id(writer: &mut CdrWriter<'_>, goal_id: &GoalId) -> Result<(), NodeError> {
    writer.write_u32(16).map_err(|_| NodeError::Serialization)?;
    for b in &goal_id.uuid {
        writer.write_u8(*b).map_err(|_| NodeError::Serialization)?;
    }
    Ok(())
}

// ============================================================================
// ActionServerCore
// ============================================================================

/// Type-agnostic action server core handling the raw-bytes protocol.
///
/// Manages active goal tracking (GoalId + status), completed result storage
/// in a fixed-size slab, and all CDR framing for the action protocol.
///
/// The typed [`ActionServer`](super::handles::ActionServer) wraps this
/// and adds `A::Goal` / `A::Feedback` / `A::Result` (de)serialization.
pub struct ActionServerCore<
    Srv,
    Pub,
    const GOAL_BUF: usize = 1024,
    const RESULT_BUF: usize = 1024,
    const FEEDBACK_BUF: usize = 1024,
    const MAX_GOALS: usize = 4,
> {
    pub(crate) send_goal_server: Srv,
    pub(crate) cancel_goal_server: Srv,
    pub(crate) get_result_server: Srv,
    pub(crate) feedback_publisher: Pub,
    pub(crate) status_publisher: Pub,
    pub(crate) active_goals: heapless::Vec<RawActiveGoal, MAX_GOALS>,
    pub(crate) completed_results: heapless::Vec<CompletedResultEntry, MAX_GOALS>,
    /// Slab storage for completed result CDR bytes.
    pub(crate) result_slab: [u8; RESULT_BUF],
    pub(crate) result_slab_used: usize,
    pub(crate) goal_buffer: [u8; GOAL_BUF],
    pub(crate) feedback_buffer: [u8; FEEDBACK_BUF],
    pub(crate) cancel_buffer: [u8; 256],
}

impl<
    Srv: ServiceServerTrait,
    Pub: Publisher,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
    const MAX_GOALS: usize,
> ActionServerCore<Srv, Pub, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>
{
    /// Try to receive a goal request from the send_goal service.
    ///
    /// Returns the parsed GoalId, sequence number, and data length.
    /// The full CDR data (including GoalId) remains in `goal_buffer`.
    pub fn try_recv_goal_request(&mut self) -> Result<Option<RawGoalRequest>, NodeError> {
        let request = self
            .send_goal_server
            .try_recv_request(&mut self.goal_buffer)
            .map_err(|_| NodeError::Transport(TransportError::ServiceRequestFailed))?;

        let request = match request {
            Some(r) => r,
            None => return Ok(None),
        };

        let data_len = request.data.len();
        let sequence_number = request.sequence_number;
        #[allow(clippy::drop_non_drop)]
        drop(request);

        let mut reader = CdrReader::new_with_header(&self.goal_buffer[..data_len])
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;

        let goal_id = read_goal_id(&mut reader)?;

        Ok(Some(RawGoalRequest {
            goal_id,
            sequence_number,
            data_len,
        }))
    }

    /// Get a reference to the goal buffer (valid after `try_recv_goal_request`).
    pub fn goal_buffer(&self) -> &[u8] {
        &self.goal_buffer
    }

    /// Accept a goal: sends the acceptance reply, adds to active goals,
    /// publishes status.
    pub fn accept_goal(&mut self, goal_id: GoalId, seq: i64) -> Result<(), NodeError> {
        // Serialize response: accepted=true + stamp (Time: sec=0, nanosec=0)
        let mut writer = CdrWriter::new_with_header(&mut self.cancel_buffer)
            .map_err(|_| NodeError::BufferTooSmall)?;
        writer.write_u8(1).map_err(|_| NodeError::Serialization)?;
        writer.write_i32(0).map_err(|_| NodeError::Serialization)?;
        writer.write_u32(0).map_err(|_| NodeError::Serialization)?;
        let reply_len = writer.position();

        self.send_goal_server
            .send_reply(seq, &self.cancel_buffer[..reply_len])
            .map_err(|_| NodeError::ServiceReplyFailed)?;

        let _ = self.active_goals.push(RawActiveGoal {
            goal_id,
            status: GoalStatus::Accepted,
        });
        let _ = self.publish_status_array();
        Ok(())
    }

    /// Reject a goal: sends the rejection reply.
    pub fn reject_goal(&mut self, seq: i64) -> Result<(), NodeError> {
        // Serialize response: accepted=false + stamp
        let mut writer = CdrWriter::new_with_header(&mut self.cancel_buffer)
            .map_err(|_| NodeError::BufferTooSmall)?;
        writer.write_u8(0).map_err(|_| NodeError::Serialization)?;
        writer.write_i32(0).map_err(|_| NodeError::Serialization)?;
        writer.write_u32(0).map_err(|_| NodeError::Serialization)?;
        let reply_len = writer.position();

        self.send_goal_server
            .send_reply(seq, &self.cancel_buffer[..reply_len])
            .map_err(|_| NodeError::ServiceReplyFailed)
    }

    /// Publish feedback with raw CDR bytes.
    ///
    /// Writes GoalId framing + raw feedback bytes into the feedback buffer
    /// and publishes.
    pub fn publish_feedback_raw(
        &mut self,
        goal_id: &GoalId,
        feedback_cdr: &[u8],
    ) -> Result<(), NodeError> {
        // GoalId framing (4 + 16 = 20 bytes) + feedback_cdr must fit in FEEDBACK_BUF
        let needed = 4 + 20 + feedback_cdr.len(); // CDR header + GoalId + feedback
        if needed > FEEDBACK_BUF {
            return Err(NodeError::BufferTooSmall);
        }

        let mut writer = CdrWriter::new_with_header(&mut self.feedback_buffer)
            .map_err(|_| NodeError::BufferTooSmall)?;

        write_goal_id(&mut writer, goal_id)?;

        // Copy raw feedback bytes directly after GoalId
        let pos = writer.position();
        if pos + feedback_cdr.len() > FEEDBACK_BUF {
            return Err(NodeError::BufferTooSmall);
        }
        self.feedback_buffer[pos..pos + feedback_cdr.len()].copy_from_slice(feedback_cdr);
        let len = pos + feedback_cdr.len();

        self.feedback_publisher
            .publish_raw(&self.feedback_buffer[..len])
            .map_err(|_| NodeError::Transport(TransportError::PublishFailed))
    }

    /// Set a goal's status and publish the updated GoalStatusArray.
    pub fn set_goal_status(&mut self, goal_id: &GoalId, status: GoalStatus) {
        for goal in &mut self.active_goals {
            if goal.goal_id.uuid == goal_id.uuid {
                goal.status = status;
                break;
            }
        }
        let _ = self.publish_status_array();
    }

    /// Complete a goal: remove from active, store raw result CDR in slab,
    /// publish status.
    pub fn complete_goal_raw(&mut self, goal_id: &GoalId, status: GoalStatus, result_cdr: &[u8]) {
        // Remove from active goals
        if let Some(pos) = self
            .active_goals
            .iter()
            .position(|g| g.goal_id.uuid == goal_id.uuid)
        {
            self.active_goals.swap_remove(pos);
        }

        // Store result CDR in the slab
        let offset = self.result_slab_used;
        let end = offset + result_cdr.len();
        if end <= RESULT_BUF {
            self.result_slab[offset..end].copy_from_slice(result_cdr);
            self.result_slab_used = end;
            let _ = self.completed_results.push(CompletedResultEntry {
                goal_id: *goal_id,
                status,
                offset,
                len: result_cdr.len(),
            });
        }

        let _ = self.publish_status_array();
    }

    /// Try to handle a cancel_goal request (type-agnostic).
    pub fn try_handle_cancel(
        &mut self,
        cancel_handler: impl FnOnce(&GoalId, GoalStatus) -> nros_core::CancelResponse,
    ) -> Result<Option<(GoalId, nros_core::CancelResponse)>, NodeError> {
        let request = self
            .cancel_goal_server
            .try_recv_request(&mut self.cancel_buffer)
            .map_err(|_| NodeError::Transport(TransportError::ServiceRequestFailed))?;

        let request = match request {
            Some(r) => r,
            None => return Ok(None),
        };

        let data_len = request.data.len();
        let sequence_number = request.sequence_number;
        #[allow(clippy::drop_non_drop)]
        drop(request);

        let mut reader = CdrReader::new_with_header(&self.cancel_buffer[..data_len])
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;

        let goal_id = read_goal_id(&mut reader)?;

        let current_status = self.find_goal_status(&goal_id);
        let response = cancel_handler(&goal_id, current_status);

        if response == nros_core::CancelResponse::Ok {
            self.set_goal_status(&goal_id, GoalStatus::Canceling);
        }

        // Serialize response: return_code (i8) + goals_canceling (sequence of GoalInfo)
        let mut writer = CdrWriter::new_with_header(&mut self.goal_buffer)
            .map_err(|_| NodeError::BufferTooSmall)?;
        writer
            .write_i8(response as i8)
            .map_err(|_| NodeError::Serialization)?;

        let num_canceling = if response == nros_core::CancelResponse::Ok {
            1u32
        } else {
            0u32
        };
        writer
            .write_u32(num_canceling)
            .map_err(|_| NodeError::Serialization)?;
        if response == nros_core::CancelResponse::Ok {
            write_goal_id(&mut writer, &goal_id)?;
            writer.write_i32(0).map_err(|_| NodeError::Serialization)?;
            writer.write_u32(0).map_err(|_| NodeError::Serialization)?;
        }
        let reply_len = writer.position();

        self.cancel_goal_server
            .send_reply(sequence_number, &self.goal_buffer[..reply_len])
            .map_err(|_| NodeError::ServiceReplyFailed)?;

        Ok(Some((goal_id, response)))
    }

    /// Try to handle a get_result request using raw bytes.
    ///
    /// For completed goals, sends the stored raw result CDR from the slab.
    /// For active/unknown goals, sends the provided `default_result_cdr` bytes.
    ///
    /// `default_result_cdr` should contain serialized result data (without CDR
    /// header or status byte) — typically `A::Result::default()` serialized.
    pub fn try_handle_get_result_raw(
        &mut self,
        default_result_cdr: &[u8],
    ) -> Result<Option<GoalId>, NodeError> {
        let request = self
            .get_result_server
            .try_recv_request(&mut self.goal_buffer)
            .map_err(|_| NodeError::Transport(TransportError::ServiceRequestFailed))?;

        let request = match request {
            Some(r) => r,
            None => return Ok(None),
        };

        let data_len = request.data.len();
        let sequence_number = request.sequence_number;
        #[allow(clippy::drop_non_drop)]
        drop(request);

        let mut reader = CdrReader::new_with_header(&self.goal_buffer[..data_len])
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;

        let goal_id = read_goal_id(&mut reader)?;

        // Look up in completed results
        let completed = self
            .completed_results
            .iter()
            .find(|c| c.goal_id.uuid == goal_id.uuid);

        if let Some(entry) = completed {
            // Completed: send status + stored result CDR from slab
            let result_bytes = &self.result_slab[entry.offset..entry.offset + entry.len];

            // Build reply in goal_buffer (we've already consumed the request)
            let mut writer = CdrWriter::new_with_header(&mut self.goal_buffer)
                .map_err(|_| NodeError::BufferTooSmall)?;
            writer
                .write_i8(entry.status as i8)
                .map_err(|_| NodeError::Serialization)?;
            let pos = writer.position();
            if pos + result_bytes.len() > GOAL_BUF {
                return Err(NodeError::BufferTooSmall);
            }
            self.goal_buffer[pos..pos + result_bytes.len()].copy_from_slice(result_bytes);
            let reply_len = pos + result_bytes.len();

            self.get_result_server
                .send_reply(sequence_number, &self.goal_buffer[..reply_len])
                .map_err(|_| NodeError::ServiceReplyFailed)?;
        } else {
            // Active or unknown: send status + default result
            let status = self
                .active_goals
                .iter()
                .find(|g| g.goal_id.uuid == goal_id.uuid)
                .map(|g| g.status)
                .unwrap_or(GoalStatus::Unknown);

            let mut writer = CdrWriter::new_with_header(&mut self.goal_buffer)
                .map_err(|_| NodeError::BufferTooSmall)?;
            writer
                .write_i8(status as i8)
                .map_err(|_| NodeError::Serialization)?;
            let pos = writer.position();
            if pos + default_result_cdr.len() > GOAL_BUF {
                return Err(NodeError::BufferTooSmall);
            }
            self.goal_buffer[pos..pos + default_result_cdr.len()]
                .copy_from_slice(default_result_cdr);
            let reply_len = pos + default_result_cdr.len();

            self.get_result_server
                .send_reply(sequence_number, &self.goal_buffer[..reply_len])
                .map_err(|_| NodeError::ServiceReplyFailed)?;
        }

        Ok(Some(goal_id))
    }

    /// Get the number of active goals.
    pub fn active_goal_count(&self) -> usize {
        self.active_goals.len()
    }

    /// Get a reference to all active goals.
    pub fn active_goals(&self) -> &[RawActiveGoal] {
        &self.active_goals
    }

    /// Find the status of a goal (active or unknown).
    pub fn find_goal_status(&self, goal_id: &GoalId) -> GoalStatus {
        self.active_goals
            .iter()
            .find(|g| g.goal_id.uuid == goal_id.uuid)
            .map(|g| g.status)
            .unwrap_or(GoalStatus::Unknown)
    }

    /// Publish the current GoalStatusArray on the status topic.
    pub fn publish_status_array(&self) -> Result<(), NodeError> {
        let mut buf = [0u8; 512];
        let mut writer =
            CdrWriter::new_with_header(&mut buf).map_err(|_| NodeError::BufferTooSmall)?;

        writer
            .write_u32(self.active_goals.len() as u32)
            .map_err(|_| NodeError::Serialization)?;

        for goal in &self.active_goals {
            let stamped = GoalStatusStamped::new(GoalInfo::with_id(goal.goal_id), goal.status);
            stamped
                .serialize(&mut writer)
                .map_err(|_| NodeError::Serialization)?;
        }

        let len = writer.position();
        self.status_publisher
            .publish_raw(&buf[..len])
            .map_err(|_| NodeError::Transport(TransportError::PublishFailed))
    }
}

// ============================================================================
// ActionClientCore
// ============================================================================

/// Type-agnostic action client core handling the raw-bytes protocol.
///
/// The typed [`ActionClient`](super::handles::ActionClient) wraps this
/// and adds serialization/deserialization at the boundary.
pub struct ActionClientCore<
    Cli,
    Sub,
    const GOAL_BUF: usize = 1024,
    const RESULT_BUF: usize = 1024,
    const FEEDBACK_BUF: usize = 1024,
> {
    pub(crate) send_goal_client: Cli,
    pub(crate) cancel_goal_client: Cli,
    pub(crate) get_result_client: Cli,
    pub(crate) feedback_subscriber: Sub,
    pub(crate) goal_buffer: [u8; GOAL_BUF],
    pub(crate) result_buffer: [u8; RESULT_BUF],
    pub(crate) feedback_buffer: [u8; FEEDBACK_BUF],
    pub(crate) goal_counter: u64,
}

impl<
    Cli: nros_rmw::ServiceClientTrait,
    Sub: Subscriber,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
> ActionClientCore<Cli, Sub, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>
{
    /// Create a new action client core from the raw transport handles.
    pub fn new(
        send_goal_client: Cli,
        cancel_goal_client: Cli,
        get_result_client: Cli,
        feedback_subscriber: Sub,
    ) -> Self {
        Self {
            send_goal_client,
            cancel_goal_client,
            get_result_client,
            feedback_subscriber,
            goal_buffer: [0u8; GOAL_BUF],
            result_buffer: [0u8; RESULT_BUF],
            feedback_buffer: [0u8; FEEDBACK_BUF],
            goal_counter: 0,
        }
    }

    /// Send a goal with raw CDR bytes. Returns the generated GoalId.
    ///
    /// The `goal_cdr` bytes are the serialized goal data (without GoalId framing).
    /// This writes GoalId + goal_cdr into the goal buffer and sends the request.
    ///
    /// After calling, use `send_goal_client` and `result_buffer` to construct
    /// a Promise for the acceptance reply.
    pub fn send_goal_raw(&mut self, goal_cdr: &[u8]) -> Result<GoalId, NodeError> {
        self.goal_counter += 1;
        let mut goal_id = GoalId::default();
        let counter_bytes = self.goal_counter.to_le_bytes();
        goal_id.uuid[..8].copy_from_slice(&counter_bytes);

        let mut writer = CdrWriter::new_with_header(&mut self.goal_buffer)
            .map_err(|_| NodeError::BufferTooSmall)?;

        write_goal_id(&mut writer, &goal_id)?;

        // Copy raw goal CDR bytes after GoalId
        let pos = writer.position();
        if pos + goal_cdr.len() > GOAL_BUF {
            return Err(NodeError::BufferTooSmall);
        }
        self.goal_buffer[pos..pos + goal_cdr.len()].copy_from_slice(goal_cdr);
        let req_len = pos + goal_cdr.len();

        self.send_goal_client
            .send_request_raw(&self.goal_buffer[..req_len])
            .map_err(|_| NodeError::ServiceRequestFailed)?;

        Ok(goal_id)
    }

    /// Try to receive feedback (non-blocking, raw bytes).
    ///
    /// Returns the GoalId and total data length. The full CDR data
    /// (including GoalId) is in `feedback_buffer`.
    pub fn try_recv_feedback_raw(&mut self) -> Result<Option<(GoalId, usize)>, NodeError> {
        let data = self
            .feedback_subscriber
            .try_recv_raw(&mut self.feedback_buffer)
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;

        let len = match data {
            Some(len) => len,
            None => return Ok(None),
        };

        let mut reader = CdrReader::new_with_header(&self.feedback_buffer[..len])
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;

        let goal_id = read_goal_id(&mut reader)?;

        Ok(Some((goal_id, len)))
    }

    /// Cancel a goal (non-blocking). Sends the cancel request.
    ///
    /// After calling, use `cancel_goal_client` and `result_buffer` to construct
    /// a Promise for the cancel response.
    pub fn send_cancel_request(&mut self, goal_id: &GoalId) -> Result<(), NodeError> {
        let mut writer = CdrWriter::new_with_header(&mut self.goal_buffer)
            .map_err(|_| NodeError::BufferTooSmall)?;

        write_goal_id(&mut writer, goal_id)?;
        writer.write_i32(0).map_err(|_| NodeError::Serialization)?;
        writer.write_u32(0).map_err(|_| NodeError::Serialization)?;

        let req_len = writer.position();

        self.cancel_goal_client
            .send_request_raw(&self.goal_buffer[..req_len])
            .map_err(|_| NodeError::ServiceRequestFailed)
    }

    /// Send a get_result request.
    ///
    /// After calling, use `get_result_client` and `result_buffer` to construct
    /// a Promise for the result response.
    pub fn send_get_result_request(&mut self, goal_id: &GoalId) -> Result<(), NodeError> {
        let mut writer = CdrWriter::new_with_header(&mut self.goal_buffer)
            .map_err(|_| NodeError::BufferTooSmall)?;

        write_goal_id(&mut writer, goal_id)?;

        let req_len = writer.position();

        self.get_result_client
            .send_request_raw(&self.goal_buffer[..req_len])
            .map_err(|_| NodeError::ServiceRequestFailed)
    }

    /// Poll for a get_result reply (non-blocking, raw bytes).
    ///
    /// Returns `Ok(Some(total_len))` if a reply arrived (data in result buffer),
    /// `Ok(None)` if no reply yet.
    ///
    /// After receiving, use [`result_buffer_ref()`](Self::result_buffer_ref)
    /// to access the raw CDR data. The layout is: CDR header (4) + status
    /// byte (1) + padding (3) + result data.
    pub fn try_recv_get_result_reply(&mut self) -> Result<Option<usize>, NodeError> {
        self.get_result_client
            .try_recv_reply_raw(&mut self.result_buffer)
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))
    }

    /// Read-only access to the result buffer (after polling a reply).
    pub fn result_buffer_ref(&self) -> &[u8] {
        &self.result_buffer
    }

    /// Read-only access to the feedback buffer (after receiving feedback).
    pub fn feedback_buffer_ref(&self) -> &[u8] {
        &self.feedback_buffer
    }
}
