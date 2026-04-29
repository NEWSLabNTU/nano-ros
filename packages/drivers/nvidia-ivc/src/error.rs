/// Errors returned by the safe Rust [`Channel`](crate::Channel) API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IvcError {
    /// The requested channel ID is not registered in the carveout.
    InvalidChannel,
    /// No frame is currently available (read returned without data).
    WouldBlock,
    /// The peer closed the channel (only meaningful on `unix-mock`;
    /// hardware IVC channels do not "close").
    Closed,
    /// A platform error from the underlying transport.
    Io,
    /// Caller-supplied buffer was too small to hold a frame.
    BufferTooSmall,
}
