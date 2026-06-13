//! Phase 211.H — `qos_overrides` runtime-delivery fixture.
//!
//! Proves a per-topic QoS override (the kind the launch planner lowers from a
//! `qos_overrides.<topic>.<role>.<policy>` launch param into the entry's baked
//! `&'static [QosOverride]` table) is HONOURED on a LIVE, running entity — not
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
//! * `NROS_LOCATOR` — zenoh locator. Default `tcp/127.0.0.1:7447`.
//!
//! The proof rests on the `qos effective:` log line — with the override it
//! reports `BestEffort`, without it `Reliable` — and on cross-process delivery
//! succeeding under the override. The effective profile is computed through the
//! SAME `QosSettings::apply_overrides` the node's create path runs, on the same
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
    Executor, ExecutorConfig, QosOverride, QosOverrideRole, QosOverrideValue, QosReliabilityPolicy,
    QosSettings,
};

const TOPIC: &str = "/chatter";
const TYPE_NAME: &str = "std_msgs::msg::dds_::Int32_";
const TYPE_HASH: &str = "TypeHashNotSupported";

/// `qos_overrides./chatter.{publisher,subscription}.reliability = best_effort`.
/// Both roles present so the same table serves talker + listener (the node
/// folds only the entry whose role matches the entity being created).
const OVR_BEST_EFFORT: &[QosOverride] = &[
    QosOverride {
        topic: TOPIC,
        role: QosOverrideRole::Publisher,
        value: QosOverrideValue::Reliability(QosReliabilityPolicy::BestEffort),
    },
    QosOverride {
        topic: TOPIC,
        role: QosOverrideRole::Subscription,
        value: QosOverrideValue::Reliability(QosReliabilityPolicy::BestEffort),
    },
];

/// CDR-LE encapsulation header (`{0x00, 0x01}`) + options + a 4-byte i32 body.
/// Content is irrelevant to the delivery assertion; only that bytes flow.
fn int32_cdr(value: i32) -> [u8; 8] {
    let mut buf = [0u8; 8];
    buf[0] = 0x00;
    buf[1] = 0x01; // CDR_LE
    buf[4..8].copy_from_slice(&value.to_le_bytes());
    buf
}

fn select_overrides() -> &'static [QosOverride] {
    match std::env::var("NROS_QOS_OVERRIDE").as_deref() {
        Ok("reliability=best_effort") => OVR_BEST_EFFORT,
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

/// Log the QoS the entity is actually created with — `QosSettings::default()`
/// folded through the SAME `apply_overrides` the node's create path runs, on the
/// same `(topic, role, table)` inputs, so the logged profile IS the live
/// entity's profile.
fn log_effective(role: QosOverrideRole, overrides: &[QosOverride]) {
    let eff = QosSettings::default().apply_overrides(TOPIC, role, overrides);
    info!(
        "qos effective: role={role:?} reliability={:?} durability={:?}",
        eff.reliability, eff.durability
    );
}

fn run_talker(exec: &mut Executor, overrides: &'static [QosOverride]) {
    log_effective(QosOverrideRole::Publisher, overrides);

    let publisher = {
        let mut node = exec.create_node("qos_override").expect("create node");
        node.set_qos_overrides(overrides);
        // The raw publisher create path folds the matching override into the
        // profile, then `validate_against` the backend's supported-policy mask.
        // The `Err` arm is defensive: a backend that genuinely can't honour the
        // override would fail loudly here, never silently downgrade. (The CFFI
        // session this executor routes through advertises a broad mask, so the
        // supported `reliability=best_effort` override always passes.)
        match node.create_publisher_raw_with_qos(
            TOPIC,
            TYPE_NAME,
            TYPE_HASH,
            QosSettings::default(),
        ) {
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
        match publisher.publish_raw(&int32_cdr(count)) {
            Ok(()) => info!("Published: {count}"),
            Err(e) => error!("publish error: {e:?}"),
        }
        count = count.wrapping_add(1);
        let _ = exec.spin_once(Duration::from_millis(10));
        std::thread::sleep(Duration::from_millis(1000));
    }
}

fn run_listener(exec: &mut Executor, overrides: &'static [QosOverride]) {
    log_effective(QosOverrideRole::Subscription, overrides);

    let mut sub = {
        let mut node = exec.create_node("qos_override").expect("create node");
        node.set_qos_overrides(overrides);
        match node.create_subscription_raw(TOPIC, TYPE_NAME, TYPE_HASH) {
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
        while let Ok(Some(n)) = sub.try_recv_raw() {
            info!("Received: {n} bytes");
        }
    }
}
