/// Error type for rclrs-style API
///
/// This will eventually replace ConnectedNodeError to match rclrs naming.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RclrsError {
    /// Failed to create context
    ContextCreationFailed,
    /// Failed to create node
    NodeCreationFailed,
    /// Failed to connect to transport
    ConnectionFailed,
    /// Failed to create publisher
    PublisherCreationFailed,
    /// Failed to create subscriber
    SubscriberCreationFailed,
    /// Failed to create service server
    ServiceServerCreationFailed,
    /// Failed to create service client
    ServiceClientCreationFailed,
    /// Failed to create action server
    ActionServerCreationFailed,
    /// Failed to create action client
    ActionClientCreationFailed,
    /// Failed to publish message
    PublishFailed,
    /// Serialization failed
    SerializationFailed,
    /// Deserialization failed
    DeserializationFailed,
    /// Buffer too small
    BufferTooSmall,
    /// Incoming message exceeded the static subscriber buffer capacity
    MessageTooLarge,
    /// No message available
    NoMessage,
    /// Service request failed
    ServiceRequestFailed,
    /// Service reply failed
    ServiceReplyFailed,
    /// Failed to start background tasks
    TaskStartFailed,
    /// Failed to poll for incoming messages
    PollFailed,
    /// Failed to send keepalive
    KeepaliveFailed,
    /// Failed to send join message
    JoinFailed,
    /// Goal was rejected
    GoalRejected,
    /// Goal not found
    GoalNotFound,
    /// Action server is full (too many active goals)
    ActionServerFull,
    /// Failed to create timer
    TimerCreationFailed,
    /// Timer not found
    TimerNotFound,
    /// Timer storage is full (too many timers)
    TimerStorageFull,
    /// Executor is full (too many nodes)
    ExecutorFull,
    /// Service call timed out
    ServiceTimeout,
    /// Service call was cancelled
    ServiceCancelled,
    /// Subscription storage is full (too many subscriptions)
    SubscriptionStorageFull,
    /// Service storage is full (too many services)
    ServiceStorageFull,
}

impl RclrsError {
    /// Return the first error from a list, or Ok if the list is empty
    ///
    /// This is useful for error handling in spin loops.
    pub fn first_error(errors: impl IntoIterator<Item = Self>) -> Result<(), Self> {
        errors.into_iter().next().map(Err).unwrap_or(Ok(()))
    }
}
