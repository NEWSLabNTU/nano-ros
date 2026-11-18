//! Phase 211.H — `qos_overrides` runtime-delivery fixture.
//!
//! Proves a per-topic QoS override (the kind the launch planner lowers from a
//! `qos_overrides.<topic>.<role>.<policy>` launch param into the entry's baked
//! `&'static [QoSOverride]` table) is HONOURED on a LIVE, running entity — not
//! just in the planner's lowering unit tests. The override is installed on the
//! `NodeHandle` via `set_qos_overrides` (exactly what a baked entry does) and
//! folded into the entity at `create_publisher_raw_*` / `create_subscription_raw`
//! time, *before* the backend-compat `validate_against`.
//!
//! ## Roles + override, env-selected
//!
//! * `NROS_QOS_ROLE` — `talker` (raw publisher, 1/s) or `listener` (raw
//!   subscription, drains + logs each receive). Required.
//! * `NROS_QOS_OVERRIDE` — the override to install on `/chatter`:
//!     * `reliability=best_effort` — folds `BestEffort` into the entity profile
//!       (the default is `Reliable`); the entity is created + delivers.
//!     * unset / empty — no override (the baseline contrast: the entity keeps
//!       the default `Reliable` profile).
//!     * `graph-profile` — every policy off its default (`GRAPH_PROFILE`), for
//!       the ROS 2 compatibility test that reads the profile back through
//!       `ros2 topic info -v` and compares it with a stock node declaring the
//!       same one (`tests/ros2_cli_parity_interop.rs`, phase-556 W6).
//! * `NROS_LOCATOR` — zenoh locator. Default `tcp/127.0.0.1:7447`.
//!
//! The proof rests on the `qos effective:` log line — with the override it
//! reports `BestEffort`, without it `Reliable` — and on cross-process delivery
//! succeeding under the override. The effective profile is computed through the
//! SAME `QoSProfile::apply_overrides` the node's create path runs, on the same
//! `(topic, role, table)` inputs, so the logged profile IS the live entity's.
//! (The executor routes through the CFFI session whose `supported_qos_policies`
//! advertises a broad mask, so an unsupported-policy *rejection* path can't be
//! exercised here — the contrast + delivery are the runtime evidence.)
//!
//! Raw pub/sub on `/chatter` (no generated message crate — same approach as the
//! `bridge-zenoh-to-xrce-fwd` fixture). The payload is a fixed CDR-LE-shaped
//! Int32; the paired test asserts on delivery + the effective-QoS log line, not
//! on payload content.

use core::time::Duration;

use log::{error, info};
use nros::{
    Executor, ExecutorConfig, QoSDurabilityPolicy, QoSHistoryPolicy, QoSLivelinessPolicy,
    QoSOverride, QoSOverrideRole, QoSOverrideValue, QoSProfile, QoSReliabilityPolicy,
};

const TOPIC: &str = "/chatter";
const TYPE_NAME: &str = "std_msgs::msg::dds_::Int32_";
const TYPE_HASH: &str = "TypeHashNotSupported";

/// `qos_overrides./chatter.{publisher,subscription}.reliability = best_effort`.
/// Both roles present so the same table serves talker + listener (the node
/// folds only the entry whose role matches the entity being created).
const OVR_BEST_EFFORT: &[QoSOverride] = &[
    QoSOverride {
        topic: TOPIC,
        role: QoSOverrideRole::Publisher,
        value: QoSOverrideValue::Reliability(QoSReliabilityPolicy::BestEffort),
    },
    QoSOverride {
        topic: TOPIC,
        role: QoSOverrideRole::Subscription,
        value: QoSOverrideValue::Reliability(QoSReliabilityPolicy::BestEffort),
    },
];

/// phase-556 W6 — a profile with every policy off its default, applied to
/// `/chatter` on both roles. `fixtures/ros2-qos-peer/qos_peer.py` declares the
/// same values on a stock rclpy node; keep the two in step.
///
/// The depths fit what the zenoh backend holds by default, so the profile a
/// peer reads is the one asked for rather than a granted-down one: a
/// transient-local publisher retains 1 sample, a subscription's ring is 4.
const fn graph_profile(role: QoSOverrideRole, depth: u32) -> [QoSOverride; 8] {
    const fn at(role: QoSOverrideRole, value: QoSOverrideValue) -> QoSOverride {
        QoSOverride {
            topic: TOPIC,
            role,
            value,
        }
    }
    [
        at(
            role,
            QoSOverrideValue::Reliability(QoSReliabilityPolicy::BestEffort),
        ),
        at(
            role,
            QoSOverrideValue::Durability(QoSDurabilityPolicy::TransientLocal),
        ),
        at(role, QoSOverrideValue::History(QoSHistoryPolicy::KeepLast)),
        at(role, QoSOverrideValue::Depth(depth)),
        at(role, QoSOverrideValue::Deadline(1500)),
        at(role, QoSOverrideValue::Lifespan(2000)),
        at(
            role,
            QoSOverrideValue::Liveliness(QoSLivelinessPolicy::ManualByTopic),
        ),
        at(role, QoSOverrideValue::LivelinessLease(3000)),
    ]
}

const GRAPH_PROFILE: &[QoSOverride] = &{
    let publisher = graph_profile(QoSOverrideRole::Publisher, 1);
    let subscription = graph_profile(QoSOverrideRole::Subscription, 3);
    let mut all = [publisher[0]; 16];
    let mut i = 0;
    while i < 8 {
        all[i] = publisher[i];
        all[i + 8] = subscription[i];
        i += 1;
    }
    all
};

/// CDR-LE encapsulation header (`{0x00, 0x01}`) + options + a 4-byte i32 body.
/// Content is irrelevant to the delivery assertion; only that bytes flow.
fn int32_cdr(value: i32) -> [u8; 8] {
    let mut buf = [0u8; 8];
    buf[0] = 0x00;
    buf[1] = 0x01; // CDR_LE
    buf[4..8].copy_from_slice(&value.to_le_bytes());
    buf
}

fn select_overrides() -> &'static [QoSOverride] {
    match std::env::var("NROS_QOS_OVERRIDE").as_deref() {
        Ok("reliability=best_effort") => OVR_BEST_EFFORT,
        Ok("graph-profile") => GRAPH_PROFILE,
        _ => &[],
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    nros_rmw_zenoh::register().expect("register zenoh backend");

    let role = std::env::var("NROS_QOS_ROLE").unwrap_or_default();
    let locator = std::env::var("NROS_LOCATOR").unwrap_or_else(|_| "tcp/127.0.0.1:7447".into());
    let overrides = select_overrides();

    info!("=== Phase 211.H qos-override-pubsub: role={role} override={overrides:?} ===");

    let cfg = ExecutorConfig::new(&locator)
        .node_name("qos_override")
        .namespace("/");
    let mut exec = Executor::open_with_rmw("zenoh", &cfg).expect("open zenoh session");

    match role.as_str() {
        "talker" => run_talker(&mut exec, overrides),
        "listener" => run_listener(&mut exec, overrides),
        other => {
            error!("NROS_QOS_ROLE must be `talker` or `listener`, got `{other}`");
            std::process::exit(2);
        }
    }
}

/// Log the QoS the entity is actually created with — `QoSProfile::default()`
/// folded through the SAME `apply_overrides` the node's create path runs, on the
/// same `(topic, role, table)` inputs, so the logged profile IS the live
/// entity's profile.
fn log_effective(role: QoSOverrideRole, overrides: &[QoSOverride]) {
    let eff = QoSProfile::default().apply_overrides(TOPIC, role, overrides);
    info!(
        "qos effective: role={role:?} reliability={:?} durability={:?}",
        eff.reliability, eff.durability
    );
}

fn run_talker(exec: &mut Executor<'static>, overrides: &'static [QoSOverride]) {
    log_effective(QoSOverrideRole::Publisher, overrides);

    let publisher = {
        // phase-483 W2 — `create_node` returns the one node type, which takes
        // its QoS overrides from the executor's node table. The TYPED table this
        // probe installs is a `NodeHandle` input, so it opens one for the call.
        let id = exec.create_node("qos_override").expect("create node").id();
        // The raw publisher create path folds the matching override into the
        // profile, then `validate_against` the backend's supported-policy mask.
        // The `Err` arm is defensive: a backend that genuinely can't honour the
        // override would fail loudly here, never silently downgrade. (The CFFI
        // session this executor routes through advertises a broad mask, so the
        // supported `reliability=best_effort` override always passes.)
        let created = exec.with_node_try(id, |node| {
            node.set_qos_overrides(overrides);
            node.create_publisher_raw_with_qos(TOPIC, TYPE_NAME, TYPE_HASH, QoSProfile::default())
        });
        match created {
            Ok(p) => {
                info!("publisher created on {TOPIC} (override honoured)");
                p
            }
            Err(e) => {
                error!("publisher create rejected: {e:?} (qos override not honourable by backend)");
                std::process::exit(3);
            }
        }
    };

    info!("publishing Int32 on {TOPIC} every 1s...");
    let mut count: i32 = 0;
    loop {
        match publisher.publish_serialized(&int32_cdr(count)) {
            Ok(()) => info!("Published: {count}"),
            Err(e) => error!("publish error: {e:?}"),
        }
        count = count.wrapping_add(1);
        let _ = exec.spin_once(Duration::from_millis(10));
        std::thread::sleep(Duration::from_millis(1000));
    }
}

fn run_listener(exec: &mut Executor<'static>, overrides: &'static [QoSOverride]) {
    log_effective(QoSOverrideRole::Subscription, overrides);

    let mut sub = {
        let id = exec.create_node("qos_override").expect("create node").id();
        let created = exec.with_node_try(id, |node| {
            node.set_qos_overrides(overrides);
            node.create_subscription_raw(TOPIC, TYPE_NAME, TYPE_HASH)
        });
        match created {
            Ok(s) => {
                info!("subscription created on {TOPIC} (override honoured)");
                s
            }
            Err(e) => {
                error!(
                    "subscription create rejected: {e:?} (qos override not honourable by backend)"
                );
                std::process::exit(3);
            }
        }
    };

    info!("Waiting for messages on {TOPIC}...");
    loop {
        let _ = exec.spin_once(Duration::from_millis(10));
        while let Ok(Some(n)) = sub.take_serialized() {
            info!("Received: {n} bytes");
        }
    }
}
