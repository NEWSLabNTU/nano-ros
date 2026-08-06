//! Receive-side `MessageInfo` observer (issue 0441).
//!
//! # Why this is a bin and not the listener example
//!
//! `zero_copy::test_zero_copy_message_info` states its subject as "MessageInfo
//! (sequence number, GID) is correctly passed through **the zero-copy
//! trampoline**" — a RECEIVE-side property. It used to assert that by grepping
//! `examples/native/rust/listener` for `seq=`, which stopped working when
//! phase-277 slimmed the example to the two lines a ROS 2 demo prints. Two
//! things made the obvious repairs wrong:
//!
//! * **The example cannot observe MessageInfo at all.** `CallbackCtx` exposes
//!   no accessor for it; the `FnMut(&M, Option<&MessageInfo>)` shape lives on
//!   the executor's `.message_info()` subscription builder, which the
//!   `Node`/`ExecutableNode` API a demo is written against never reaches. So
//!   there was nothing to un-slim — the line had never come from the receive
//!   path.
//! * **Adding a `cfg` branch to the example would break the portability
//!   gate.** phase-338 W1 asserts every platform copy of a program is
//!   byte-identical after normalization; a zero-copy branch in the native copy
//!   diverges it from six others, and adding it to all seven would put a
//!   native-only feature in firmware examples for a test's benefit.
//!
//! Retargeting at the PUBLISHER's trace (what issue 0429 did for `nano2nano`)
//! would have gone green while silently no longer testing the receive path.
//! So the assertion moves to a purpose-built observer instead, where the
//! builder that carries `MessageInfo` is in scope and no user-facing example
//! has to grow a test-shaped branch.
//!
//! # Output contract
//!
//! One line per received message:
//!
//! ```text
//! seq=<publication_sequence_number> gid=<hex> ts=<source_timestamp_nanos>
//! ```
//!
//! `seq` and `gid` are what the test asserts on (monotonic sequence, stable
//! GID). Built with and without `unstable-zenoh-api`, the same line must appear
//! either way — that equality IS the trampoline check, since the feature
//! changes which receive path the runtime takes and nothing else.

use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::String as StringMsg;

fn main() {
    env_logger::init();
    nros_board_linux::register_linked_rmw();

    // Contains "Listener" for the same reason int32-sink's banner does: the
    // e2e spawn helpers key readiness off that word.
    info!("nros MessageInfo Observer Listener (test fixture)");

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("listener");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    let nid = executor
        .node_builder("listener")
        .build()
        .expect("Failed to build node");

    let topic: &'static str = match std::env::var("NROS_SUB_TOPIC") {
        Ok(t) if !t.is_empty() => Box::leak(t.into_boxed_str()),
        _ => "/chatter",
    };

    executor
        .node_mut(nid)
        .subscription(topic)
        .typed::<StringMsg>()
        // The whole point of this bin: the rclrs-shaped callback that receives
        // per-message `MessageInfo` alongside the payload.
        .message_info()
        .build(move |msg: &StringMsg, info: Option<&nros::MessageInfo>| {
            match info {
                Some(mi) => {
                    let gid = mi
                        .publisher_gid()
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<std::string::String>();
                    // TWO lines per message, on purpose. `I heard:` is the
                    // standard chatter-delivery marker
                    // (`nros_tests::output::LISTENER_LOG_PREFIX`) that the
                    // plain delivery assertions count, so this bin is a drop-in
                    // for a listener; `seq=` carries what only the
                    // MessageInfo-shaped callback can see
                    // (`nros_tests::output::MESSAGE_INFO_LOG_PREFIX`).
                    info!("I heard: [{}]", msg.data);
                    info!(
                        "seq={} gid={} ts={}",
                        mi.publication_sequence_number(),
                        gid,
                        mi.source_timestamp().to_nanos(),
                    );
                }
                // Fail LOUD rather than printing a line the test would parse as
                // a successful observation. A `None` here means the receive
                // path delivered no attachment, which is exactly the regression
                // this bin exists to catch — and a silent skip would read as
                // "no messages" (issue 0441's own failure mode).
                None => {
                    error!(
                        "MessageInfo ABSENT for a message on {topic} — the receive path \
                         delivered no attachment (payload=[{}])",
                        msg.data
                    );
                }
            }
        })
        .expect("Failed to add subscription");

    info!("Subscriber created for topic: {topic}");
    info!("Waiting for messages with MessageInfo on {topic}...");

    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}
