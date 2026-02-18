//! Entity wrapper types for the embedded executor.

use core::marker::PhantomData;

use nros_core::{CdrReader, CdrWriter, Deserialize, RosAction, RosMessage, RosService, Serialize};
use nros_rmw::{Publisher, ServiceClientTrait, ServiceServerTrait, Subscriber, TransportError};

use super::types::{DEFAULT_TX_BUF, NodeError};

// ============================================================================
// EmbeddedPublisher
// ============================================================================

/// Typed publisher handle.
pub struct EmbeddedPublisher<M, P> {
    pub(crate) handle: P,
    pub(crate) _phantom: PhantomData<M>,
}

impl<M: RosMessage, P: Publisher> EmbeddedPublisher<M, P> {
    /// Publish a message using the default buffer size.
    pub fn publish(&self, msg: &M) -> Result<(), NodeError> {
        self.publish_with_buffer::<DEFAULT_TX_BUF>(msg)
    }

    /// Publish a message with a custom buffer size.
    pub fn publish_with_buffer<const BUF: usize>(&self, msg: &M) -> Result<(), NodeError> {
        let mut buffer = [0u8; BUF];
        let mut writer =
            CdrWriter::new_with_header(&mut buffer).map_err(|_| NodeError::BufferTooSmall)?;
        msg.serialize(&mut writer)
            .map_err(|_| NodeError::Serialization)?;
        let len = writer.position();
        self.handle
            .publish_raw(&buffer[..len])
            .map_err(|_| NodeError::Transport(TransportError::PublishFailed))
    }

    /// Publish raw CDR-encoded data (must include CDR header).
    pub fn publish_raw(&self, data: &[u8]) -> Result<(), NodeError> {
        self.handle
            .publish_raw(data)
            .map_err(|_| NodeError::Transport(TransportError::PublishFailed))
    }
}

// ============================================================================
// Subscription
// ============================================================================

/// Typed subscription handle with internal receive buffer.
pub struct Subscription<M, Sub, const RX_BUF: usize = 1024> {
    pub(crate) handle: Sub,
    pub(crate) buffer: [u8; RX_BUF],
    pub(crate) _phantom: PhantomData<M>,
}

impl<M: RosMessage, Sub: Subscriber, const RX_BUF: usize> Subscription<M, Sub, RX_BUF> {
    /// Try to receive a typed message (non-blocking).
    pub fn try_recv(&mut self) -> Result<Option<M>, NodeError> {
        match self
            .handle
            .try_recv_raw(&mut self.buffer)
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?
        {
            Some(len) => {
                let mut reader = CdrReader::new_with_header(&self.buffer[..len])
                    .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;
                let msg = M::deserialize(&mut reader)
                    .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;
                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }

    /// Try to receive raw CDR-encoded data (non-blocking).
    pub fn try_recv_raw(&mut self) -> Result<Option<usize>, NodeError> {
        self.handle
            .try_recv_raw(&mut self.buffer)
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))
    }

    /// Get the receive buffer (valid after `try_recv_raw`).
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    /// Check if data is available without consuming it.
    pub fn has_data(&self) -> bool {
        self.handle.has_data()
    }

    /// Process the received message in-place without copying.
    pub fn process_in_place(&mut self, f: impl FnOnce(&M)) -> Result<bool, NodeError> {
        let mut deser_err = false;
        let processed = self
            .handle
            .process_raw_in_place(|raw| {
                match CdrReader::new_with_header(raw).and_then(|mut r| M::deserialize(&mut r)) {
                    Ok(msg) => f(&msg),
                    Err(_) => deser_err = true,
                }
            })
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;

        if deser_err {
            return Err(NodeError::Transport(TransportError::DeserializationError));
        }
        Ok(processed)
    }
}

// ============================================================================
// EmbeddedServiceServer
// ============================================================================

/// Typed service server handle with internal buffers.
pub struct EmbeddedServiceServer<
    Svc: RosService,
    Srv,
    const REQ_BUF: usize = 1024,
    const REPLY_BUF: usize = 1024,
> {
    pub(crate) handle: Srv,
    pub(crate) req_buffer: [u8; REQ_BUF],
    pub(crate) reply_buffer: [u8; REPLY_BUF],
    pub(crate) _phantom: PhantomData<Svc>,
}

impl<Svc: RosService, Srv: ServiceServerTrait, const REQ_BUF: usize, const REPLY_BUF: usize>
    EmbeddedServiceServer<Svc, Srv, REQ_BUF, REPLY_BUF>
where
    Srv::Error: From<TransportError>,
{
    /// Handle an incoming service request.
    ///
    /// Returns `Ok(true)` if a request was handled, `Ok(false)` if none available.
    pub fn handle_request(
        &mut self,
        handler: impl FnOnce(&Svc::Request) -> Svc::Reply,
    ) -> Result<bool, NodeError> {
        self.handle
            .handle_request::<Svc>(&mut self.req_buffer, &mut self.reply_buffer, handler)
            .map_err(|_| NodeError::ServiceReplyFailed)
    }

    /// Handle a request with a heap-allocated reply (for large response types).
    ///
    /// Returns `Ok(true)` if a request was handled, `Ok(false)` if none available.
    #[cfg(feature = "alloc")]
    pub fn handle_request_boxed(
        &mut self,
        handler: impl FnOnce(&Svc::Request) -> alloc::boxed::Box<Svc::Reply>,
    ) -> Result<bool, NodeError> {
        self.handle
            .handle_request_boxed::<Svc>(&mut self.req_buffer, &mut self.reply_buffer, handler)
            .map_err(|_| NodeError::ServiceReplyFailed)
    }

    /// Check if a request is available.
    pub fn has_request(&self) -> bool {
        self.handle.has_request()
    }
}

// ============================================================================
// EmbeddedServiceClient
// ============================================================================

/// Typed service client handle with internal buffers.
pub struct EmbeddedServiceClient<
    Svc: RosService,
    Cli,
    const REQ_BUF: usize = 1024,
    const REPLY_BUF: usize = 1024,
> {
    pub(crate) handle: Cli,
    pub(crate) req_buffer: [u8; REQ_BUF],
    pub(crate) reply_buffer: [u8; REPLY_BUF],
    pub(crate) _phantom: PhantomData<Svc>,
}

impl<Svc: RosService, Cli: ServiceClientTrait, const REQ_BUF: usize, const REPLY_BUF: usize>
    EmbeddedServiceClient<Svc, Cli, REQ_BUF, REPLY_BUF>
where
    Cli::Error: From<TransportError>,
{
    /// Call the service with a typed request and wait for reply.
    pub fn call(&mut self, request: &Svc::Request) -> Result<Svc::Reply, NodeError> {
        self.handle
            .call::<Svc>(request, &mut self.req_buffer, &mut self.reply_buffer)
            .map_err(|_| NodeError::ServiceRequestFailed)
    }
}

// ============================================================================
// Action types
// ============================================================================

/// Active goal tracking for action server.
#[derive(Clone)]
pub struct EmbeddedActiveGoal<A: RosAction> {
    /// Goal ID.
    pub goal_id: nros_core::GoalId,
    /// Current status.
    pub status: nros_core::GoalStatus,
    /// The goal data.
    pub goal: A::Goal,
}

/// Completed goal with result.
pub struct EmbeddedCompletedGoal<A: RosAction> {
    /// Goal ID.
    pub goal_id: nros_core::GoalId,
    /// Final status.
    pub status: nros_core::GoalStatus,
    /// The result data.
    pub result: A::Result,
}

// ============================================================================
// EmbeddedActionServer
// ============================================================================

/// Typed action server with goal state management.
pub struct EmbeddedActionServer<
    A: RosAction,
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
    pub(crate) _status_publisher: Pub,
    pub(crate) active_goals: heapless::Vec<EmbeddedActiveGoal<A>, MAX_GOALS>,
    pub(crate) completed_goals: heapless::Vec<EmbeddedCompletedGoal<A>, MAX_GOALS>,
    pub(crate) goal_buffer: [u8; GOAL_BUF],
    pub(crate) result_buffer: [u8; RESULT_BUF],
    pub(crate) feedback_buffer: [u8; FEEDBACK_BUF],
    pub(crate) cancel_buffer: [u8; 256],
}

impl<
    A: RosAction,
    Srv: ServiceServerTrait,
    Pub: Publisher,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
    const MAX_GOALS: usize,
> EmbeddedActionServer<A, Srv, Pub, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>
{
    /// Try to accept a new goal.
    ///
    /// Checks for incoming send_goal requests. If one is available, calls the
    /// handler to decide acceptance. Returns the goal ID if accepted.
    pub fn try_accept_goal(
        &mut self,
        goal_handler: impl FnOnce(&A::Goal) -> nros_core::GoalResponse,
    ) -> Result<Option<nros_core::GoalId>, NodeError>
    where
        A::Goal: Clone,
    {
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

        // Read goal_id (UUID as CDR sequence)
        let uuid_len = reader
            .read_u32()
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?
            as usize;
        let mut goal_id = nros_core::GoalId::default();
        if uuid_len == 16 {
            for byte in &mut goal_id.uuid {
                *byte = reader
                    .read_u8()
                    .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;
            }
        }

        let goal = A::Goal::deserialize(&mut reader)
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;

        let response = goal_handler(&goal);
        let accepted = response.is_accepted();

        // Serialize response: accepted (bool) + stamp (Time)
        let mut writer = CdrWriter::new_with_header(&mut self.result_buffer)
            .map_err(|_| NodeError::BufferTooSmall)?;
        writer
            .write_u8(if accepted { 1 } else { 0 })
            .map_err(|_| NodeError::Serialization)?;
        writer.write_i32(0).map_err(|_| NodeError::Serialization)?;
        writer.write_u32(0).map_err(|_| NodeError::Serialization)?;
        let reply_len = writer.position();

        self.send_goal_server
            .send_reply(sequence_number, &self.result_buffer[..reply_len])
            .map_err(|_| NodeError::ServiceReplyFailed)?;

        if accepted {
            let _ = self.active_goals.push(EmbeddedActiveGoal {
                goal_id,
                status: nros_core::GoalStatus::Accepted,
                goal,
            });
            Ok(Some(goal_id))
        } else {
            Ok(None)
        }
    }

    /// Publish feedback for a goal.
    pub fn publish_feedback(
        &mut self,
        goal_id: &nros_core::GoalId,
        feedback: &A::Feedback,
    ) -> Result<(), NodeError> {
        let mut writer = CdrWriter::new_with_header(&mut self.feedback_buffer)
            .map_err(|_| NodeError::BufferTooSmall)?;

        writer.write_u32(16).map_err(|_| NodeError::Serialization)?;
        for b in &goal_id.uuid {
            writer.write_u8(*b).map_err(|_| NodeError::Serialization)?;
        }

        feedback
            .serialize(&mut writer)
            .map_err(|_| NodeError::Serialization)?;

        let len = writer.position();
        self.feedback_publisher
            .publish_raw(&self.feedback_buffer[..len])
            .map_err(|_| NodeError::Transport(TransportError::PublishFailed))
    }

    /// Set a goal's status.
    pub fn set_goal_status(&mut self, goal_id: &nros_core::GoalId, status: nros_core::GoalStatus) {
        for goal in &mut self.active_goals {
            if goal.goal_id.uuid == goal_id.uuid {
                goal.status = status;
                break;
            }
        }
    }

    /// Complete a goal and store the result.
    pub fn complete_goal(
        &mut self,
        goal_id: &nros_core::GoalId,
        status: nros_core::GoalStatus,
        result: A::Result,
    ) {
        if let Some(pos) = self
            .active_goals
            .iter()
            .position(|g| g.goal_id.uuid == goal_id.uuid)
        {
            self.active_goals.swap_remove(pos);
        }

        let _ = self.completed_goals.push(EmbeddedCompletedGoal {
            goal_id: *goal_id,
            status,
            result,
        });
    }

    /// Try to handle a cancel_goal request.
    pub fn try_handle_cancel(
        &mut self,
        cancel_handler: impl FnOnce(
            &nros_core::GoalId,
            nros_core::GoalStatus,
        ) -> nros_core::CancelResponse,
    ) -> Result<Option<(nros_core::GoalId, nros_core::CancelResponse)>, NodeError> {
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

        let mut goal_id = nros_core::GoalId::default();
        let uuid_len = reader.read_u32().unwrap_or(0) as usize;
        if uuid_len == 16 {
            for byte in &mut goal_id.uuid {
                *byte = reader.read_u8().unwrap_or(0);
            }
        }

        let current_status = self
            .active_goals
            .iter()
            .find(|g| g.goal_id.uuid == goal_id.uuid)
            .map(|g| g.status)
            .unwrap_or(nros_core::GoalStatus::Unknown);

        let response = cancel_handler(&goal_id, current_status);

        if response == nros_core::CancelResponse::Ok {
            self.set_goal_status(&goal_id, nros_core::GoalStatus::Canceling);
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
            writer.write_u32(16).map_err(|_| NodeError::Serialization)?;
            for b in &goal_id.uuid {
                writer.write_u8(*b).map_err(|_| NodeError::Serialization)?;
            }
            writer.write_i32(0).map_err(|_| NodeError::Serialization)?;
            writer.write_u32(0).map_err(|_| NodeError::Serialization)?;
        }
        let reply_len = writer.position();

        self.cancel_goal_server
            .send_reply(sequence_number, &self.goal_buffer[..reply_len])
            .map_err(|_| NodeError::ServiceReplyFailed)?;

        Ok(Some((goal_id, response)))
    }

    /// Try to handle a get_result request.
    pub fn try_handle_get_result(&mut self) -> Result<Option<nros_core::GoalId>, NodeError>
    where
        A::Result: Clone,
    {
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

        let mut goal_id = nros_core::GoalId::default();
        let uuid_len = reader.read_u32().unwrap_or(0) as usize;
        if uuid_len == 16 {
            for byte in &mut goal_id.uuid {
                *byte = reader.read_u8().unwrap_or(0);
            }
        }

        let completed = self
            .completed_goals
            .iter()
            .find(|g| g.goal_id.uuid == goal_id.uuid);

        let active = self
            .active_goals
            .iter()
            .find(|g| g.goal_id.uuid == goal_id.uuid);

        let mut writer = CdrWriter::new_with_header(&mut self.result_buffer)
            .map_err(|_| NodeError::BufferTooSmall)?;

        if let Some(completed_goal) = completed {
            writer
                .write_i8(completed_goal.status as i8)
                .map_err(|_| NodeError::Serialization)?;
            completed_goal
                .result
                .serialize(&mut writer)
                .map_err(|_| NodeError::Serialization)?;
        } else if let Some(active_goal) = active {
            writer
                .write_i8(active_goal.status as i8)
                .map_err(|_| NodeError::Serialization)?;
            A::Result::default()
                .serialize(&mut writer)
                .map_err(|_| NodeError::Serialization)?;
        } else {
            writer
                .write_i8(nros_core::GoalStatus::Unknown as i8)
                .map_err(|_| NodeError::Serialization)?;
            A::Result::default()
                .serialize(&mut writer)
                .map_err(|_| NodeError::Serialization)?;
        }

        let reply_len = writer.position();
        self.get_result_server
            .send_reply(sequence_number, &self.result_buffer[..reply_len])
            .map_err(|_| NodeError::ServiceReplyFailed)?;

        Ok(Some(goal_id))
    }

    /// Get a reference to an active goal.
    pub fn get_goal(&self, goal_id: &nros_core::GoalId) -> Option<&EmbeddedActiveGoal<A>> {
        self.active_goals
            .iter()
            .find(|g| g.goal_id.uuid == goal_id.uuid)
    }

    /// Get all active goals.
    pub fn active_goals(&self) -> &[EmbeddedActiveGoal<A>] {
        &self.active_goals
    }

    /// Get the number of active goals.
    pub fn active_goal_count(&self) -> usize {
        self.active_goals.len()
    }
}

// ============================================================================
// EmbeddedActionClient
// ============================================================================

/// Typed action client handle.
pub struct EmbeddedActionClient<
    A: RosAction,
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
    pub(crate) _phantom: PhantomData<A>,
}

impl<
    A: RosAction,
    Cli: ServiceClientTrait,
    Sub: Subscriber,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
> EmbeddedActionClient<A, Cli, Sub, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>
{
    /// Send a goal to the action server.
    pub fn send_goal(&mut self, goal: &A::Goal) -> Result<nros_core::GoalId, NodeError> {
        self.goal_counter += 1;
        let mut goal_id = nros_core::GoalId::default();
        let counter_bytes = self.goal_counter.to_le_bytes();
        goal_id.uuid[..8].copy_from_slice(&counter_bytes);

        let mut writer = CdrWriter::new_with_header(&mut self.goal_buffer)
            .map_err(|_| NodeError::BufferTooSmall)?;

        writer.write_u32(16).map_err(|_| NodeError::Serialization)?;
        for b in &goal_id.uuid {
            writer.write_u8(*b).map_err(|_| NodeError::Serialization)?;
        }

        goal.serialize(&mut writer)
            .map_err(|_| NodeError::Serialization)?;

        let req_len = writer.position();

        let reply_len = self
            .send_goal_client
            .call_raw(&self.goal_buffer[..req_len], &mut self.result_buffer)
            .map_err(|_| NodeError::ServiceRequestFailed)?;

        let mut reader = CdrReader::new_with_header(&self.result_buffer[..reply_len])
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;

        let accepted = reader.read_u8().unwrap_or(0) != 0;

        if accepted {
            Ok(goal_id)
        } else {
            Err(NodeError::ServiceRequestFailed)
        }
    }

    /// Try to receive feedback (non-blocking).
    pub fn try_recv_feedback(
        &mut self,
    ) -> Result<Option<(nros_core::GoalId, A::Feedback)>, NodeError> {
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

        let mut goal_id = nros_core::GoalId::default();
        let uuid_len = reader.read_u32().unwrap_or(0) as usize;
        if uuid_len == 16 {
            for byte in &mut goal_id.uuid {
                *byte = reader.read_u8().unwrap_or(0);
            }
        }

        let feedback = A::Feedback::deserialize(&mut reader)
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;

        Ok(Some((goal_id, feedback)))
    }

    /// Cancel a goal.
    pub fn cancel_goal(
        &mut self,
        goal_id: &nros_core::GoalId,
    ) -> Result<nros_core::CancelResponse, NodeError> {
        let mut writer = CdrWriter::new_with_header(&mut self.goal_buffer)
            .map_err(|_| NodeError::BufferTooSmall)?;

        writer.write_u32(16).map_err(|_| NodeError::Serialization)?;
        for b in &goal_id.uuid {
            writer.write_u8(*b).map_err(|_| NodeError::Serialization)?;
        }
        writer.write_i32(0).map_err(|_| NodeError::Serialization)?;
        writer.write_u32(0).map_err(|_| NodeError::Serialization)?;

        let req_len = writer.position();

        let reply_len = self
            .cancel_goal_client
            .call_raw(&self.goal_buffer[..req_len], &mut self.result_buffer)
            .map_err(|_| NodeError::ServiceRequestFailed)?;

        let mut reader = CdrReader::new_with_header(&self.result_buffer[..reply_len])
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;

        let return_code = reader.read_i8().unwrap_or(2);
        Ok(nros_core::CancelResponse::from_i8(return_code).unwrap_or_default())
    }

    /// Get the result of a completed goal.
    pub fn get_result(
        &mut self,
        goal_id: &nros_core::GoalId,
    ) -> Result<(nros_core::GoalStatus, A::Result), NodeError> {
        let mut writer = CdrWriter::new_with_header(&mut self.goal_buffer)
            .map_err(|_| NodeError::BufferTooSmall)?;

        writer.write_u32(16).map_err(|_| NodeError::Serialization)?;
        for b in &goal_id.uuid {
            writer.write_u8(*b).map_err(|_| NodeError::Serialization)?;
        }

        let req_len = writer.position();

        let reply_len = self
            .get_result_client
            .call_raw(&self.goal_buffer[..req_len], &mut self.result_buffer)
            .map_err(|_| NodeError::ServiceRequestFailed)?;

        let mut reader = CdrReader::new_with_header(&self.result_buffer[..reply_len])
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;

        let status_code = reader.read_i8().unwrap_or(0);
        let status = nros_core::GoalStatus::from_i8(status_code).unwrap_or_default();

        let result = A::Result::deserialize(&mut reader)
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;

        Ok((status, result))
    }
}
