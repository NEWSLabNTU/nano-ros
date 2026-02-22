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
    /// Call the service (non-blocking). Returns a [`Promise`] that can be polled.
    ///
    /// Use with [`Executor::spin_once()`] to drive I/O while waiting:
    ///
    /// ```ignore
    /// let mut promise = client.call(&request)?;
    /// loop {
    ///     executor.spin_once(10);
    ///     if let Some(reply) = promise.try_recv()? {
    ///         break;
    ///     }
    /// }
    /// ```
    pub fn call(
        &mut self,
        request: &Svc::Request,
    ) -> Result<Promise<'_, Svc::Reply, Cli>, NodeError> {
        // Serialize request into req_buffer
        let mut writer = CdrWriter::new_with_header(&mut self.req_buffer)
            .map_err(|_| NodeError::BufferTooSmall)?;
        request
            .serialize(&mut writer)
            .map_err(|_| NodeError::Serialization)?;
        let req_len = writer.position();

        // Send the request (non-blocking)
        self.handle
            .send_request_raw(&self.req_buffer[..req_len])
            .map_err(|_| NodeError::ServiceRequestFailed)?;

        Ok(Promise {
            handle: &mut self.handle,
            reply_buffer: &mut self.reply_buffer,
            parse: cdr_deserialize_reply::<Svc>,
        })
    }
}

// ============================================================================
// Promise
// ============================================================================

/// A pending reply from a non-blocking service or action call.
///
/// Poll with [`try_recv()`](Promise::try_recv) to check for the reply.
/// Implements [`Future`](core::future::Future) for use with async executors.
pub struct Promise<'a, T, Cli: ServiceClientTrait> {
    pub(crate) handle: &'a mut Cli,
    pub(crate) reply_buffer: &'a mut [u8],
    pub(crate) parse: fn(&[u8]) -> Result<T, NodeError>,
}

impl<T, Cli: ServiceClientTrait> Promise<'_, T, Cli> {
    /// Try to receive the reply (non-blocking).
    ///
    /// Returns `Ok(Some(reply))` if the reply has arrived,
    /// `Ok(None)` if still pending.
    pub fn try_recv(&mut self) -> Result<Option<T>, NodeError> {
        match self
            .handle
            .try_recv_reply_raw(self.reply_buffer)
            .map_err(|_| NodeError::ServiceRequestFailed)?
        {
            Some(len) => {
                let reply = (self.parse)(&self.reply_buffer[..len])?;
                Ok(Some(reply))
            }
            None => Ok(None),
        }
    }
}

impl<T, Cli: ServiceClientTrait> Promise<'_, T, Cli> {
    /// Block until the reply arrives, spinning the executor.
    ///
    /// Internally calls `executor.spin_once()` in a loop until the reply
    /// arrives or `timeout_ms` is exhausted. This is equivalent to the
    /// manual spin+poll loop pattern but more ergonomic for simple use cases.
    ///
    /// No borrow conflict: `executor` and `self` (which borrows the standalone
    /// client) are disjoint objects.
    ///
    /// # Errors
    ///
    /// Returns [`NodeError::Timeout`] if the reply does not arrive within
    /// `timeout_ms` milliseconds.
    pub fn wait<S: nros_rmw::Session, const M: usize, const C: usize>(
        &mut self,
        executor: &mut super::Executor<S, M, C>,
        timeout_ms: u64,
    ) -> Result<T, NodeError> {
        let spin_interval_ms = 10u64;
        let max_spins = (timeout_ms / spin_interval_ms).max(1);
        for _ in 0..max_spins {
            executor.spin_once(spin_interval_ms as i32);
            if let Some(result) = self.try_recv()? {
                return Ok(result);
            }
        }
        Err(NodeError::Timeout)
    }
}

impl<T, Cli: ServiceClientTrait> core::future::Future for Promise<'_, T, Cli> {
    type Output = Result<T, NodeError>;

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let this = self.get_mut();
        match this.try_recv() {
            Ok(Some(reply)) => core::task::Poll::Ready(Ok(reply)),
            Ok(None) => {
                cx.waker().wake_by_ref();
                core::task::Poll::Pending
            }
            Err(e) => core::task::Poll::Ready(Err(e)),
        }
    }
}

/// Deserialize a CDR-encoded service reply.
fn cdr_deserialize_reply<Svc: RosService>(data: &[u8]) -> Result<Svc::Reply, NodeError> {
    let mut reader =
        CdrReader::new_with_header(data).map_err(|_| NodeError::ServiceRequestFailed)?;
    Svc::Reply::deserialize(&mut reader).map_err(|_| NodeError::ServiceRequestFailed)
}

// ============================================================================
// Action types
// ============================================================================

/// Active goal tracking for action server.
#[derive(Clone)]
pub struct ActiveGoal<A: RosAction> {
    /// Goal ID.
    pub goal_id: nros_core::GoalId,
    /// Current status.
    pub status: nros_core::GoalStatus,
    /// The goal data.
    pub goal: A::Goal,
}

/// Completed goal with result.
pub struct CompletedGoal<A: RosAction> {
    /// Goal ID.
    pub goal_id: nros_core::GoalId,
    /// Final status.
    pub status: nros_core::GoalStatus,
    /// The result data.
    pub result: A::Result,
}

// ============================================================================
// ActionServer
// ============================================================================

/// Typed action server with goal state management.
///
/// Wraps [`ActionServerCore`](super::action_core::ActionServerCore) for
/// raw-bytes protocol handling, adding typed goal/feedback/result
/// serialization at the boundary.
pub struct ActionServer<
    A: RosAction,
    Srv,
    Pub,
    const GOAL_BUF: usize = 1024,
    const RESULT_BUF: usize = 1024,
    const FEEDBACK_BUF: usize = 1024,
    const MAX_GOALS: usize = 4,
> {
    pub(crate) core: super::action_core::ActionServerCore<
        Srv,
        Pub,
        GOAL_BUF,
        RESULT_BUF,
        FEEDBACK_BUF,
        MAX_GOALS,
    >,
    /// Typed goal data parallel to `core.active_goals`.
    pub(crate) typed_goals: heapless::Vec<A::Goal, MAX_GOALS>,
    /// Completed goals with typed results.
    pub(crate) completed_goals: heapless::Vec<CompletedGoal<A>, MAX_GOALS>,
}

impl<
    A: RosAction,
    Srv: ServiceServerTrait,
    Pub: Publisher,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
    const MAX_GOALS: usize,
> ActionServer<A, Srv, Pub, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>
{
    /// Try to accept a new goal.
    ///
    /// Checks for incoming send_goal requests. If one is available, calls the
    /// handler to decide acceptance. Returns the goal ID if accepted.
    pub fn try_accept_goal(
        &mut self,
        goal_handler: impl FnOnce(&nros_core::GoalId, &A::Goal) -> nros_core::GoalResponse,
    ) -> Result<Option<nros_core::GoalId>, NodeError>
    where
        A::Goal: Clone,
    {
        let raw_req = self.core.try_recv_goal_request()?;
        let raw_req = match raw_req {
            Some(r) => r,
            None => return Ok(None),
        };

        // Deserialize the goal from the buffer (GoalId already extracted by core)
        let mut reader = CdrReader::new_with_header(&self.core.goal_buffer()[..raw_req.data_len])
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;
        // Skip past the GoalId (4-byte len + 16 UUID bytes)
        let _ = reader.read_u32();
        for _ in 0..16 {
            let _ = reader.read_u8();
        }
        let goal = A::Goal::deserialize(&mut reader)
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;

        let response = goal_handler(&raw_req.goal_id, &goal);
        let accepted = response.is_accepted();

        if accepted {
            self.core
                .accept_goal(raw_req.goal_id, raw_req.sequence_number)?;
            let _ = self.typed_goals.push(goal);
            Ok(Some(raw_req.goal_id))
        } else {
            self.core.reject_goal(raw_req.sequence_number)?;
            Ok(None)
        }
    }

    /// Publish feedback for a goal.
    pub fn publish_feedback(
        &mut self,
        goal_id: &nros_core::GoalId,
        feedback: &A::Feedback,
    ) -> Result<(), NodeError> {
        // Serialize feedback into a temp buffer (without CDR header or GoalId)
        let mut tmp = [0u8; FEEDBACK_BUF];
        let mut writer = CdrWriter::new(&mut tmp);
        feedback
            .serialize(&mut writer)
            .map_err(|_| NodeError::Serialization)?;
        let feedback_len = writer.position();

        self.core
            .publish_feedback_raw(goal_id, &tmp[..feedback_len])
    }

    /// Set a goal's status.
    ///
    /// Also publishes the updated `GoalStatusArray` on the status topic.
    pub fn set_goal_status(&mut self, goal_id: &nros_core::GoalId, status: nros_core::GoalStatus) {
        self.core.set_goal_status(goal_id, status);
    }

    /// Complete a goal and store the result.
    ///
    /// Also publishes the updated `GoalStatusArray` on the status topic.
    pub fn complete_goal(
        &mut self,
        goal_id: &nros_core::GoalId,
        status: nros_core::GoalStatus,
        result: A::Result,
    ) {
        // Serialize result for the core slab
        let mut tmp = [0u8; RESULT_BUF];
        let mut writer = CdrWriter::new(&mut tmp);
        let result_len = match result.serialize(&mut writer) {
            Ok(()) => writer.position(),
            Err(_) => 0,
        };

        // Remove typed goal parallel to core's active_goals removal
        if let Some(pos) = self
            .core
            .active_goals()
            .iter()
            .position(|g| g.goal_id.uuid == goal_id.uuid)
        {
            self.typed_goals.swap_remove(pos);
        }

        self.core
            .complete_goal_raw(goal_id, status, &tmp[..result_len]);

        let _ = self.completed_goals.push(CompletedGoal {
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
        self.core.try_handle_cancel(cancel_handler)
    }

    /// Try to handle a get_result request.
    pub fn try_handle_get_result(&mut self) -> Result<Option<nros_core::GoalId>, NodeError>
    where
        A::Result: Clone + Default,
    {
        // Serialize default result for non-completed goals
        let mut default_buf = [0u8; RESULT_BUF];
        let mut writer = CdrWriter::new(&mut default_buf);
        let default_len = match A::Result::default().serialize(&mut writer) {
            Ok(()) => writer.position(),
            Err(_) => 0,
        };

        self.core
            .try_handle_get_result_raw(&default_buf[..default_len])
    }

    /// Get a reference to an active goal.
    pub fn get_goal(&self, goal_id: &nros_core::GoalId) -> Option<ActiveGoal<A>>
    where
        A::Goal: Clone,
    {
        self.core
            .active_goals()
            .iter()
            .enumerate()
            .find(|(_, g)| g.goal_id.uuid == goal_id.uuid)
            .map(|(i, raw)| ActiveGoal {
                goal_id: raw.goal_id,
                status: raw.status,
                goal: self.typed_goals[i].clone(),
            })
    }

    /// Get the number of active goals.
    pub fn active_goal_count(&self) -> usize {
        self.core.active_goal_count()
    }
}

// ============================================================================
// ActionClient
// ============================================================================

/// Typed action client handle.
///
/// Wraps [`ActionClientCore`](super::action_core::ActionClientCore) for
/// raw-bytes protocol handling, adding typed goal/feedback/result
/// serialization at the boundary.
pub struct ActionClient<
    A: RosAction,
    Cli,
    Sub,
    const GOAL_BUF: usize = 1024,
    const RESULT_BUF: usize = 1024,
    const FEEDBACK_BUF: usize = 1024,
> {
    pub(crate) core:
        super::action_core::ActionClientCore<Cli, Sub, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>,
    pub(crate) _phantom: PhantomData<A>,
}

impl<
    A: RosAction,
    Cli: ServiceClientTrait,
    Sub: Subscriber,
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
> ActionClient<A, Cli, Sub, GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>
{
    /// Send a goal (non-blocking). Returns the goal ID and a [`Promise`] for acceptance.
    ///
    /// The promise resolves to `true` if accepted, `false` if rejected.
    pub fn send_goal(
        &mut self,
        goal: &A::Goal,
    ) -> Result<(nros_core::GoalId, Promise<'_, bool, Cli>), NodeError> {
        // Serialize goal into a temp buffer (without CDR header or GoalId)
        let mut tmp = [0u8; GOAL_BUF];
        let mut writer = CdrWriter::new(&mut tmp);
        goal.serialize(&mut writer)
            .map_err(|_| NodeError::Serialization)?;
        let goal_len = writer.position();

        let goal_id = self.core.send_goal_raw(&tmp[..goal_len])?;

        Ok((
            goal_id,
            Promise {
                handle: &mut self.core.send_goal_client,
                reply_buffer: &mut self.core.result_buffer,
                parse: parse_goal_accepted,
            },
        ))
    }

    /// Try to receive feedback (non-blocking).
    pub fn try_recv_feedback(
        &mut self,
    ) -> Result<Option<(nros_core::GoalId, A::Feedback)>, NodeError> {
        let (goal_id, len) = match self.core.try_recv_feedback_raw()? {
            Some(v) => v,
            None => return Ok(None),
        };

        // Deserialize feedback from the core's feedback buffer (after GoalId)
        let mut reader = CdrReader::new_with_header(&self.core.feedback_buffer[..len])
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;
        // Skip GoalId (4 + 16 bytes)
        let _ = reader.read_u32();
        for _ in 0..16 {
            let _ = reader.read_u8();
        }

        let feedback = A::Feedback::deserialize(&mut reader)
            .map_err(|_| NodeError::Transport(TransportError::DeserializationError))?;

        Ok(Some((goal_id, feedback)))
    }

    /// Cancel a goal (non-blocking). Returns a [`Promise`] for the cancel response.
    pub fn cancel_goal(
        &mut self,
        goal_id: &nros_core::GoalId,
    ) -> Result<Promise<'_, nros_core::CancelResponse, Cli>, NodeError> {
        self.core.send_cancel_request(goal_id)?;

        Ok(Promise {
            handle: &mut self.core.cancel_goal_client,
            reply_buffer: &mut self.core.result_buffer,
            parse: parse_cancel_response,
        })
    }

    /// Get the result of a completed goal (non-blocking). Returns a [`Promise`].
    pub fn get_result(
        &mut self,
        goal_id: &nros_core::GoalId,
    ) -> Result<Promise<'_, (nros_core::GoalStatus, A::Result), Cli>, NodeError> {
        self.core.send_get_result_request(goal_id)?;

        Ok(Promise {
            handle: &mut self.core.get_result_client,
            reply_buffer: &mut self.core.result_buffer,
            parse: parse_result_response::<A>,
        })
    }
}

/// Parse a goal acceptance response (bool).
fn parse_goal_accepted(data: &[u8]) -> Result<bool, NodeError> {
    let mut reader =
        CdrReader::new_with_header(data).map_err(|_| NodeError::ServiceRequestFailed)?;
    let accepted = reader.read_u8().unwrap_or(0) != 0;
    Ok(accepted)
}

/// Parse a cancel response.
fn parse_cancel_response(data: &[u8]) -> Result<nros_core::CancelResponse, NodeError> {
    let mut reader =
        CdrReader::new_with_header(data).map_err(|_| NodeError::ServiceRequestFailed)?;
    let return_code = reader.read_i8().unwrap_or(2);
    Ok(nros_core::CancelResponse::from_i8(return_code).unwrap_or_default())
}

/// Parse an action result response (status + result).
fn parse_result_response<A: RosAction>(
    data: &[u8],
) -> Result<(nros_core::GoalStatus, A::Result), NodeError> {
    let mut reader =
        CdrReader::new_with_header(data).map_err(|_| NodeError::ServiceRequestFailed)?;
    let status_code = reader.read_i8().unwrap_or(0);
    let status = nros_core::GoalStatus::from_i8(status_code).unwrap_or_default();
    let result =
        A::Result::deserialize(&mut reader).map_err(|_| NodeError::ServiceRequestFailed)?;
    Ok((status, result))
}
