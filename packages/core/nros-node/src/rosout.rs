//! `/rosout` — the PUBLISHER half of the log bridge.
//!
//! Ledger row `c:logging_rosout_enabled` (log.json). Upstream, every rcl node
//! republishes its log records on `/rosout` as `rcl_interfaces/msg/Log`, and
//! that topic is how an operator reads a running robot: `ros2 topic echo
//! /rosout`, `rqt_console`, every launch-side aggregator. A nano-ros node was
//! silent there.
//!
//! The near half — getting a record out of the logging call and onto a thread
//! that may block — is [`nros_log::rosout`]: a bounded static ring, no
//! allocation, no RMW. This module is the far half: drain that ring and put
//! each record on the wire.
//!
//! # The executor owns the writer, one per node
//!
//! In an image built with the `rosout` capability the
//! EXECUTOR creates one `/rosout` publisher per node, on the spin after the
//! node is built, and every spin pumps the queue into them: a record goes to
//! the writer of the node whose logger raised it (a child logger's goes to
//! its node's), and a record no node's logger claims goes to the first
//! node's. That is upstream's shape -- rcl creates a `/rosout` publisher in
//! every node's constructor -- so a program logs and does nothing else:
//!
//! ```ignore
//! let mut executor = Executor::open(&cfg)?;
//! let node = executor.create_node("talker")?;
//! let logger = node.logger();
//! loop {
//!     nros_log::log_info!(logger, "tick");
//!     executor.spin_once(budget); // creates the writer once, then pumps
//! }
//! ```
//!
//! The writer is [`qos_bounded`]: VOLATILE, a documented divergence from
//! upstream's TRANSIENT_LOCAL (user ruling 2026-11-13). A node turns its own
//! off with `Executor::disable_rosout_on`; the writer is dropped with the
//! executor, and at `close()`.
//!
//! **Counted, not conjured.** A publisher is an ENTITY: it reaches
//! `EntityInventory::derive`'s counts, the zenoh pools and the sizing
//! descriptor. The `rosout` capability is a declared feature of the image
//! (`[system] features`, or the stock node interfaces), and the inventory
//! prices [`ROSOUT_PUBLISHERS`] per node from it, so every entity the image
//! creates is one the sizing counted.
//!
//! **The explicit spelling is deprecated.** Previously the application
//! created the publisher and called [`pump`]. Both still compile and still
//! work (the executor's pump runs first, so the explicit one usually finds
//! the queue empty), but they add a second `/rosout` writer to the node, so
//! [`pump`] is `#[deprecated]`, as is C++'s `rclcpp::rosout::Publisher`. C's
//! `rclc_rosout_publisher_init` / `rclc_rosout_pump` are NOT: a legacy
//! `rclc_node_init_default` node is not in the executor's node table, so the
//! executor has no writer for it, and the explicit spelling is its only road.
//! An executor with no writer of its own leaves the queue for that pump.
//!
//! # Why this cannot re-enter
//!
//! Three separate reasons, and the first two are already load-bearing before
//! this module exists:
//!
//! 1. `nros_log`'s dispatcher holds its dispatch guard across every
//!    `sink.log()` call, so a sink that logs cannot recurse: its record is
//!    held and delivered after the current one, a bounded number of passes per
//!    dispatch.
//! 2. The SINK does not publish. It copies into a `.bss` slot and returns, so
//!    no logging call ever enters the RMW stack. This is what keeps issue
//!    0589's shape — a Zephyr native_sim image dying by stack exhaustion with
//!    no message — off the logging path entirely.
//! 3. [`pump`] runs OUTSIDE the dispatcher, where neither of the above
//!    applies, so `nros_log::rosout::drain` raises its own `PUMPING` flag for
//!    the whole drain and the queue refuses anything raised under it. That is
//!    the amplification a transport-that-logs would otherwise cause: not
//!    recursion, but one record published, one record logged, one record
//!    queued, forever. It is counted
//!    ([`nros_log::rosout::suppressed`]) rather than hidden.

use nros_log::{Severity, rosout as queue};
use nros_rcl_interfaces::msg::Log;
use nros_rmw::{
    QoSDurabilityPolicy, QoSHistoryPolicy, QoSProfile, QoSReliabilityPolicy, TransportError,
};

use crate::executor::{EmbeddedPublisher, NodeError};

pub use nros_log::rosout::{RosoutScope, dropped, enabled, scope, suppressed};

/// The [`RosoutScope`] this image's ROS release implies.
///
/// `ros-iron` / `ros-jazzy`: node loggers and their `get_child` descendants,
/// as `rcl_logging_rosout_add_sublogger` makes it upstream. `ros-humble`, and
/// an image that names no release: node loggers only — Humble's rcl publishes
/// only through a publisher correlated with a node's logger, and Humble is the
/// release this tree's interop tests run against.
pub const RELEASE_SCOPE: RosoutScope = if cfg!(any(feature = "ros-iron", feature = "ros-jazzy")) {
    RosoutScope::NodeLoggersAndDescendants
} else {
    RosoutScope::NodeLoggers
};

/// Start queueing records for `/rosout`, scoped to what this image's ROS
/// release publishes ([`RELEASE_SCOPE`]). `nros_log::rosout::enable` with the
/// scope set first; returns its answer.
pub fn enable() -> bool {
    nros_log::rosout::set_scope(RELEASE_SCOPE);
    nros_log::rosout::enable()
}

/// The topic. Absolute and un-namespaced, as upstream's is.
pub const TOPIC: &str = "/rosout";

/// The publishers the `rosout` capability claims PER NODE:
/// the one `/rosout` writer the executor creates for each node
/// (`create_rosout_pub` in `executor/spin.rs`, the one creation site
/// `check-infra-queryable-counts` holds this to). The host CLI's entity
/// inventory, the shared sizing library and the zenoh liveliness default
/// mirror it (none can read this crate) and multiply it by the node count.
pub const ROSOUT_PUBLISHERS: usize = 1;

/// Records the executor's pump has handed to a `/rosout` writer since boot.
static PUBLISHED: portable_atomic::AtomicUsize = portable_atomic::AtomicUsize::new(0);

/// How many records the executor's per-node `/rosout` writers have published
/// since boot (saturating at `usize::MAX`). With [`dropped`] and
/// [`suppressed`] it is the bridge's whole accounting.
#[must_use]
pub fn published() -> usize {
    PUBLISHED.load(portable_atomic::Ordering::Relaxed)
}

/// Every `rcl_interfaces/msg/Log` string field is a `heapless::String<256>`,
/// so no field can carry more than this however wide the logging format
/// buffer is. Joined to the generated type by
/// `the_field_cap_matches_the_generated_message`, which is the only way this
/// crate can notice codegen changing it.
const FIELD_CAP: usize = 256;

/// One CDR string of at most `cap` bytes: 4 alignment + 4 length + the bytes +
/// the NUL upstream's IDL mapping requires.
const fn cdr_string(cap: usize) -> usize {
    4 + 4 + cap + 1 + 3
}

/// The publish buffer one `Log` needs in the WORST case: **844 bytes**,
/// derived from the message, against a maximal record MEASURED at 820
/// (`a_maximal_log_encodes_inside_the_derived_buffer`).
///
/// Derived rather than inherited, and the reason is not the one you would
/// guess. I expected a maximal `Log` to overflow `EmbeddedPublisher::publish`'s
/// `DEFAULT_RX_BUF_SIZE`; it does not — 820 < 1024, so today's default would
/// have worked and the test that asserted otherwise failed. What makes the
/// default wrong here is that it is not a constant: `DEFAULT_RX_BUF_SIZE` is
/// whatever the image declared through `NROS_SUBSCRIPTION_BUFFER_SIZE`
/// (`nros-node/build.rs`), i.e. a bound over the types THAT image's
/// endpoints carry. An image whose own topics are 128-byte structs may
/// legitimately declare 256, and `/rosout` — which no launch file declares —
/// would then silently lose exactly the longest and most interesting records
/// to `NodeError::BufferTooSmall`. This buffer is a property of `Log`, so it
/// is computed from `Log`.
pub const TX_BUF: usize = 4          // CDR encapsulation header
    + 4                              // XCDR2 DHEADER
    + 8                              // builtin_interfaces/Time
    + 4                              // level + padding
    + cdr_string(FIELD_CAP)          // name
    + cdr_string(FIELD_CAP)          // msg
    + cdr_string(FIELD_CAP)          // file
    + cdr_string(0)                  // function — always empty, see `pump`
    + 8; // line + DHEADER close

/// `rcl_qos_profile_rosout_default` — KEEP_LAST(1000), RELIABLE,
/// TRANSIENT_LOCAL, 10 s lifespan. **Upstream's, exactly**, and the same four
/// policies `rclcpp::RosoutQoS` transcribes on our C++ surface
/// (`packages/api/nros-cpp/include/rclcpp/qos.hpp`, ledger row `cpp:RosoutQoS`).
///
/// # What it costs here, measured
///
/// TRANSIENT_LOCAL is not free on an embedded image. On the zenoh backend a
/// transient-local publisher takes one slot in the retention pool
/// (`MAX_TL_PUBLISHERS`) **and** one cache queryable out of
/// `ZPICO_MAX_QUERYABLES`, which defaults to 8 on an embedded build while
/// `[param_services]` (6) and `[lifecycle]` (5) already claim eleven between
/// them. And KEEP_LAST(1000) over a type whose by-value
/// size is ~1 KB is a megabyte of retained history.
///
/// So this profile is CORRECT and it is not the default here. Use it on a
/// host, or on a target that has declared the slot. Use [`qos_bounded`]
/// otherwise, and read its doc for what the operator loses.
#[must_use]
pub fn qos() -> QoSProfile {
    QoSProfile {
        history: QoSHistoryPolicy::KeepLast,
        depth: 1000,
        reliability: QoSReliabilityPolicy::Reliable,
        durability: QoSDurabilityPolicy::TransientLocal,
        lifespan_ms: 10_000,
        ..QoSProfile::default()
    }
}

/// The profile an image with a fixed pool budget can actually afford:
/// KEEP_LAST([`nros_log::rosout::rosout_depth`]), RELIABLE, **VOLATILE**, same
/// 10 s lifespan.
///
/// # The divergence, stated
///
/// Upstream's `/rosout` is TRANSIENT_LOCAL, so a tool that starts AFTER the
/// node still sees the node's last 1000 records. With this profile it sees
/// only what is published from the moment it subscribes — **the boot story is
/// lost to a late subscriber**. That is the whole of the difference, and it is
/// bought for zero queryable slots and zero retention pool.
///
/// MEASURED, because the obvious worry is that a volatile publisher is simply
/// invisible: it is not. `ros2 topic echo /rosout` requests VOLATILE and
/// auto-downgrades when it finds a mixed set of publishers, printing
/// "Some, but not all, publishers are offering TRANSIENT_LOCAL. Falling back
/// to VOLATILE as it will connect to all publishers", and then prints our
/// records. A TRANSIENT_LOCAL *subscriber* would not match, which is the one
/// direction RxO refuses.
#[must_use]
pub fn qos_bounded() -> QoSProfile {
    QoSProfile {
        history: QoSHistoryPolicy::KeepLast,
        depth: queue::rosout_depth() as u32,
        reliability: QoSReliabilityPolicy::Reliable,
        durability: QoSDurabilityPolicy::Volatile,
        lifespan_ms: 10_000,
        ..QoSProfile::default()
    }
}

/// Drain every queued record onto `publisher`. Returns how many reached
/// `publish`, or the FIRST transport error with the count that preceded it.
///
/// Call it from the same loop as `spin_once`. It never blocks on the queue and
/// it never allocates; the one buffer it needs is [`TX_BUF`] bytes of the
/// caller's stack.
///
/// Losses since the previous pump are reported ON `/rosout` ITSELF, as one
/// WARN record from the logger `nros_rosout`, so an operator watching the
/// topic learns about the gap on the channel the gap is in rather than only
/// from a counter nobody reads.
#[deprecated(
    note = "the executor now creates a /rosout publisher for every node and publishes the \
            log queue from spin_once; remove this call and the explicit /rosout publisher"
)]
pub fn pump(publisher: &EmbeddedPublisher<Log>) -> Result<usize, (usize, NodeError)> {
    pump_with(|msg| publisher.publish_with_buffer::<TX_BUF>(msg))
}

/// [`pump`] for a surface that holds an UNTYPED publisher — the C and C++
/// handles, which publish CDR bytes (`rclc_publish_serialized`,
/// `nros_cpp_publish_raw`). Each record is encoded into [`TX_BUF`] bytes of
/// this frame and handed to `send`; `send` answering `Err` is a transport
/// failure, reported exactly as [`pump`] reports one.
///
/// One encoder for every language: the C and C++ bridges call this rather than
/// growing their own `Log` mapping, so `fill`'s field rules —
/// clipping, the empty `function`, the monotonic stamp — hold on all three.
pub fn pump_raw(
    mut send: impl FnMut(&[u8]) -> Result<(), ()>,
) -> Result<usize, (usize, NodeError)> {
    use nros_core::Serialize as _;
    pump_with(|msg| {
        let mut buffer = [0u8; TX_BUF];
        let mut writer = crate::tx_writer(&mut buffer).map_err(|_| NodeError::BufferTooSmall)?;
        msg.serialize(&mut writer)
            .map_err(|_| NodeError::Serialization)?;
        let len = writer.position();
        send(&buffer[..len]).map_err(|()| NodeError::Transport(TransportError::PublishFailed))
    })
}

/// The executor's pump: [`pump_raw`] that also hands `send` the record's
/// LOGGER NAME, so the caller can route it to the writer of the node that
/// logged it. `send` answering `Err` is a transport failure, reported as
/// [`pump`] reports one. Counts every record that reached a writer into
/// [`published`]. No allocation; [`TX_BUF`] bytes of this frame per record.
pub(crate) fn pump_routed(
    mut send: impl FnMut(&str, &[u8]) -> Result<(), ()>,
) -> Result<usize, (usize, NodeError)> {
    use nros_core::Serialize as _;
    let result = pump_with(|msg| {
        let mut buffer = [0u8; TX_BUF];
        let mut writer = crate::tx_writer(&mut buffer).map_err(|_| NodeError::BufferTooSmall)?;
        msg.serialize(&mut writer)
            .map_err(|_| NodeError::Serialization)?;
        let len = writer.position();
        send(msg.name.as_str(), &buffer[..len])
            .map_err(|()| NodeError::Transport(TransportError::PublishFailed))
    });
    let sent = match &result {
        Ok(n) => *n,
        Err((n, _)) => *n,
    };
    let _ = PUBLISHED.fetch_update(
        portable_atomic::Ordering::Relaxed,
        portable_atomic::Ordering::Relaxed,
        |v| Some(v.saturating_add(sent)),
    );
    result
}

/// Whether the queue holds anything the pump would publish: a record, or a
/// loss it reports on `/rosout` itself. The cheap test `spin_once` makes
/// before it enters the pump's frame.
#[inline]
pub(crate) fn has_work() -> bool {
    !queue::is_empty() || queue::dropped() > 0 || queue::suppressed() > 0
}

/// `rcl_interfaces/msg/Log`'s wire identity, for a surface that creates the
/// `/rosout` publisher by NAME rather than by Rust type (C, C++).
pub const TYPE_NAME: &str = <Log as nros_core::RosMessage>::TYPE_NAME;
/// See [`TYPE_NAME`].
pub const TYPE_HASH: &str = <Log as nros_core::RosMessage>::TYPE_HASH;

/// The drain both [`pump`] and [`pump_raw`] run; `publish` puts one message on
/// the wire.
fn pump_with(
    mut publish: impl FnMut(&Log) -> Result<(), NodeError>,
) -> Result<usize, (usize, NodeError)> {
    let mut msg = Log::default();
    let mut sent = 0usize;
    let mut failure: Option<NodeError> = None;
    queue::drain(&mut |record| {
        if failure.is_some() {
            // Stop encoding once the transport has refused: the remaining
            // records are still drained (the queue must not wedge) but there
            // is nothing useful to do with them.
            return;
        }
        fill(&mut msg, record);
        match publish(&msg) {
            Ok(()) => sent += 1,
            Err(e) => failure = Some(e),
        }
    });

    let (dropped, suppressed) = queue::take_losses();
    if failure.is_none() && (dropped > 0 || suppressed > 0) {
        note_losses(&mut msg, dropped, suppressed);
        match publish(&msg) {
            Ok(()) => sent += 1,
            Err(e) => failure = Some(e),
        }
    }

    match failure {
        Some(e) => Err((sent, e)),
        None => Ok(sent),
    }
}

/// Overwrite `msg` from `record`. Reuses one `Log` across the whole drain —
/// it is ~1 KB by value and a fresh one per record would put that on the stack
/// `n` times.
fn fill(msg: &mut Log, record: &nros_log::Record<'_>) {
    // The record's clock is the platform MONOTONIC one
    // (`nros_platform_clock_ns`, and `0` in a build without nros-log's
    // platform clock — `cfg(nros_log_clock)`, issue 1037), NOT a ROS system clock: an RTOS image has no
    // wall time to offer. `rqt_console` sorts by this and displays it as a
    // time of day, which will read as an offset from the epoch. Stated here
    // and in the ledger row rather than silently zeroed, because a zero stamp
    // and an uptime stamp are both wrong in different ways and only one of
    // them tells the operator anything.
    msg.stamp.sec = (record.timestamp_ns / 1_000_000_000) as i32;
    msg.stamp.nanosec = (record.timestamp_ns % 1_000_000_000) as u32;
    msg.level = record.severity.rcutils_level();
    set(&mut msg.name, record.logger_name);
    set(&mut msg.msg, record.message);
    set(&mut msg.file, record.file);
    // `function` stays empty: `core::file!()` and `core::line!()` have no
    // `core::function!()` twin, so there is nothing true to put here. Empty is
    // the honest answer; a repeat of `file` would not be.
    msg.function.clear();
    msg.line = record.line;
}

fn note_losses(msg: &mut Log, dropped: usize, suppressed: usize) {
    msg.stamp.sec = 0;
    msg.stamp.nanosec = 0;
    msg.level = Severity::Warn.rcutils_level();
    set(&mut msg.name, "nros_rosout");
    msg.msg.clear();
    let mut buf = nros_log::FormatBuffer::new();
    use core::fmt::Write as _;
    let _ = write!(
        buf,
        "/rosout lost {dropped} record(s) to a full queue and suppressed \
         {suppressed} raised while publishing"
    );
    set(&mut msg.msg, buf.as_str());
    set(&mut msg.file, core::file!());
    msg.function.clear();
    msg.line = core::line!();
}

/// Replace `dst`'s contents with as much of `src` as fits, never splitting a
/// character. `heapless::String::push_str` is all-or-nothing, so a field one
/// byte too long would otherwise arrive EMPTY.
fn set(dst: &mut heapless::String<FIELD_CAP>, src: &str) {
    dst.clear();
    let mut end = core::cmp::min(src.len(), FIELD_CAP);
    while end > 0 && !src.is_char_boundary(end) {
        end -= 1;
    }
    let _ = dst.push_str(&src[..end]);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one join this crate can make to the generated message: if codegen
    /// ever widens or narrows the string fields, [`FIELD_CAP`] and [`TX_BUF`]
    /// are both wrong and this says so.
    #[test]
    fn the_field_cap_matches_the_generated_message() {
        let msg = Log::default();
        assert_eq!(msg.name.capacity(), FIELD_CAP);
        assert_eq!(msg.msg.capacity(), FIELD_CAP);
        assert_eq!(msg.file.capacity(), FIELD_CAP);
        assert_eq!(msg.function.capacity(), FIELD_CAP);
    }

    /// A maximal record ENCODES inside [`TX_BUF`], and the derivation is
    /// TIGHT — a buffer padded to a round number would pass this without
    /// saying anything about the message.
    #[test]
    fn a_maximal_log_encodes_inside_the_derived_buffer() {
        use nros_core::Serialize;
        let mut msg = Log::default();
        let long = "x".repeat(FIELD_CAP);
        set(&mut msg.name, &long);
        set(&mut msg.msg, &long);
        set(&mut msg.file, &long);
        msg.line = u32::MAX;

        let mut big = [0u8; TX_BUF];
        let mut w = crate::tx_writer(&mut big).expect("writer");
        msg.serialize(&mut w)
            .expect("a maximal Log must fit TX_BUF");
        let encoded = w.position();

        assert!(
            encoded <= TX_BUF,
            "a maximal Log must fit TX_BUF: encoded {encoded}, TX_BUF {TX_BUF}"
        );
        assert!(
            TX_BUF - encoded < 64,
            "TX_BUF is meant to be derived from the message, not rounded up: \
             encoded {encoded}, TX_BUF {TX_BUF}"
        );
    }

    /// Severity crosses as rcutils's number, not as `nros_log`'s compact
    /// discriminant — `rqt_console` filters on `Log.level == 40`, and a `4`
    /// there reads as a level between DEBUG and INFO.
    #[test]
    fn the_wire_level_is_rcutils_numbering() {
        let mut msg = Log::default();
        // `rcl_interfaces/msg/Log.msg`'s own constants. Spelled out rather
        // than imported because the generated crate re-exports only the STRUCT
        // (`packages/interfaces/generated/humble/nros-rcl-interfaces/src/msg/mod.rs`
        // is `mod log; pub use log::Log;`), so `msg::log::INFO` is not
        // reachable from outside the crate. Not worked around in generated
        // code; recorded here so the next reader knows why the numbers are
        // literal.
        for (severity, expected) in [
            (Severity::Debug, 10u8),
            (Severity::Info, 20),
            (Severity::Warn, 30),
            (Severity::Error, 40),
            (Severity::Fatal, 50),
        ] {
            fill(
                &mut msg,
                &nros_log::Record {
                    severity,
                    logger_name: "n",
                    message: "m",
                    file: "f",
                    line: 1,
                    timestamp_ns: 0,
                },
            );
            assert_eq!(msg.level, expected, "{severity:?}");
        }
    }

    /// The profile the executor's per-node writer uses is VOLATILE and
    /// RELIABLE (user ruling 2026-11-13: a documented divergence from
    /// upstream's TRANSIENT_LOCAL, which on zenoh costs a queryable slot).
    #[test]
    fn the_runtime_writer_profile_is_volatile_and_reliable() {
        let qos = qos_bounded();
        assert_eq!(qos.durability, QoSDurabilityPolicy::Volatile);
        assert_eq!(qos.reliability, QoSReliabilityPolicy::Reliable);
    }

    /// A field one byte over the message's capacity is CLIPPED, never dropped.
    /// `push_str` is all-or-nothing, so the naive spelling delivers an empty
    /// `msg` for exactly the longest lines.
    #[test]
    fn an_oversized_field_clips_instead_of_vanishing() {
        let mut msg = Log::default();
        let over = "é".repeat(FIELD_CAP); // 2 bytes each: twice the capacity
        set(&mut msg.msg, &over);
        assert!(!msg.msg.is_empty());
        assert!(msg.msg.len() <= FIELD_CAP);
        assert!(msg.msg.chars().all(|c| c == 'é'));
    }

    /// A record's monotonic nanoseconds split into the message's `sec` /
    /// `nanosec` without losing the sub-second part.
    #[test]
    fn the_stamp_splits_monotonic_nanoseconds() {
        let mut msg = Log::default();
        fill(
            &mut msg,
            &nros_log::Record {
                severity: Severity::Info,
                logger_name: "n",
                message: "m",
                file: "f",
                line: 1,
                timestamp_ns: 12_000_000_345,
            },
        );
        assert_eq!(msg.stamp.sec, 12);
        assert_eq!(msg.stamp.nanosec, 345);
    }
}
