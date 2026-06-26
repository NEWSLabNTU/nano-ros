//! TOML-driven entrypoint (Phase 128.G).
//!
//! Lets a binary that links one or more RMW backends boot directly
//! from a `nros-bridge.toml` file — no backend name appears in source.
//! Selection lives entirely in the manifest (Cargo `[dependencies]`)
//! plus the config file.
//!
//! Note: this is the **bridge** config, distinct from the orchestration
//! `nros.toml` (Phase 126 component/system config). The bridge file is
//! named `nros-bridge.toml` to avoid the collision (Phase 172.L).
//!
//! # Schema
//!
//! ```toml
//! # nros-bridge.toml — sibling of the binary
//! [[node]]
//! name    = "field"
//! rmw     = "zenoh"
//! locator = "tcp/10.0.0.1:7447"
//!
//! [[node]]
//! name    = "control"
//! rmw     = "dds"
//! locator = "domain=0"
//!
//! [[bridge]]
//! type      = "std_msgs/Int32"
//! type_hash = "RIHS01_..."
//! from      = { node = "field",   topic = "/sensor/raw" }
//! to        = { node = "control", topic = "/sensor/raw" }
//! ```
//!
//! Run via [`run_from_config`]:
//!
//! ```ignore
//! fn main() -> Result<(), nros_bridge::ConfigError> {
//!     nros_bridge::run_from_config("nros-bridge.toml")
//! }
//! ```
//!
//! The runtime opens one session per `[[node]]`, registers each Node
//! against the matching backend, instantiates a [`PubSubBridge`] per
//! `[[bridge]]`, and spins forever. Any error (parse, open, wiring)
//! short-circuits with [`ConfigError`].

extern crate alloc;

use alloc::{boxed::Box, format, string::String, vec::Vec};
use core::fmt;
use std::{fs, path::Path};

use nros_node::executor::{Executor, SessionSpec};

use crate::PubSubBridge;

/// Top-level error type for [`run_from_config`]. Variants are
/// boxed-string for diagnostic clarity; the runtime never recovers
/// from these.
#[derive(Debug)]
pub enum ConfigError {
    Io(String),
    Parse(String),
    UnknownNode(String),
    OpenSession(String),
    BuildNode(String),
    BuildEntity(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io(s) => write!(f, "config io: {s}"),
            ConfigError::Parse(s) => write!(f, "config parse: {s}"),
            ConfigError::UnknownNode(s) => write!(f, "bridge references unknown node: {s}"),
            ConfigError::OpenSession(s) => write!(f, "open_multi failed: {s}"),
            ConfigError::BuildNode(s) => write!(f, "create_node_on failed: {s}"),
            ConfigError::BuildEntity(s) => write!(f, "create entity failed: {s}"),
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(serde::Deserialize, Debug)]
struct ConfigFile {
    #[serde(default)]
    node: Vec<NodeCfg>,
    #[serde(default)]
    bridge: Vec<BridgeCfg>,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct NodeCfg {
    name: String,
    rmw: String,
    #[serde(default)]
    locator: String,
    #[serde(default)]
    domain_id: u32,
    #[serde(default)]
    namespace: String,
}

#[derive(serde::Deserialize, Debug)]
struct BridgeCfg {
    #[serde(rename = "type")]
    type_name: String,
    #[serde(default)]
    type_hash: String,
    from: BridgeEndpointCfg,
    to: BridgeEndpointCfg,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct BridgeEndpointCfg {
    node: String,
    topic: String,
}

/// Load `path` and run an Executor bound to whatever nodes / bridges
/// the file declares. Blocks until the executor exits.
///
/// Backend names in the file (`rmw = "zenoh"` etc.) MUST match
/// backends that this binary's manifest pulled in. Mismatches surface
/// as [`ConfigError::OpenSession`] when `Executor::open_multi`
/// rejects the spec.
pub fn run_from_config(path: impl AsRef<Path>) -> Result<(), ConfigError> {
    let raw = fs::read_to_string(path.as_ref())
        .map_err(|e| ConfigError::Io(format!("{}: {e}", path.as_ref().display())))?;
    run_from_config_str(&raw)
}

/// phase-267 W1c/C4 — run a bridge from the config CONTENTS (not a file path).
///
/// The `nros::main!` macro `include_str!`s the `nros-bridge.toml` that
/// `nros sync` generated (so the config is embedded in the binary — no runtime
/// file path to get wrong) and hands the contents here. Identical wiring to
/// [`run_from_config`]; only the source differs.
pub fn run_from_config_str(raw: &str) -> Result<(), ConfigError> {
    let cfg: ConfigFile = toml::from_str(raw).map_err(|e| ConfigError::Parse(format!("{e}")))?;

    // Build one SessionSpec per [[node]]. The first node's session is
    // the primary; the rest open as extras.
    if cfg.node.is_empty() {
        return Err(ConfigError::Parse(
            "config must declare at least one [[node]]".into(),
        ));
    }
    let mut specs: Vec<SessionSpec<'_>> = Vec::with_capacity(cfg.node.len());
    for n in &cfg.node {
        let spec = SessionSpec::new(n.rmw.as_str(), n.locator.as_str())
            .domain_id(n.domain_id)
            .node_name(n.name.as_str())
            .namespace(if n.namespace.is_empty() {
                "/"
            } else {
                n.namespace.as_str()
            });
        specs.push(spec);
    }

    let mut exec =
        Executor::open_multi(&specs).map_err(|e| ConfigError::OpenSession(format!("{e:?}")))?;

    // Register every Node so create_node_on can resolve them. We
    // intentionally drop each `Node` immediately after creation —
    // bridges work off raw subscription / publisher handles
    // constructed via `create_node_on` calls below.
    for n in &cfg.node {
        let _ = exec
            .create_node_on(n.name.as_str(), n.rmw.as_str())
            .map_err(|e| ConfigError::BuildNode(format!("{}: {e:?}", n.name)))?;
    }

    // Build every bridge. Each bridge re-derives the per-Node session
    // via `create_node_on` (idempotent — node_builder dedupes on
    // rmw + locator) then creates the source subscription / dest
    // publisher and hands them to `PubSubBridge::new`.
    let mut bridges: Vec<Box<dyn PumpableBridge>> = Vec::new();
    for b in &cfg.bridge {
        let src_rmw = node_rmw(&cfg.node, &b.from.node)?;
        let dst_rmw = node_rmw(&cfg.node, &b.to.node)?;

        let mut src_node = exec
            .create_node_on(b.from.node.as_str(), src_rmw)
            .map_err(|e| ConfigError::BuildNode(format!("{}: {e:?}", b.from.node)))?;
        let sub = src_node
            .create_subscription_raw(
                b.from.topic.as_str(),
                b.type_name.as_str(),
                b.type_hash.as_str(),
            )
            .map_err(|e| ConfigError::BuildEntity(format!("sub on {}: {e:?}", b.from.node)))?;
        drop(src_node);

        let mut dst_node = exec
            .create_node_on(b.to.node.as_str(), dst_rmw)
            .map_err(|e| ConfigError::BuildNode(format!("{}: {e:?}", b.to.node)))?;
        let pubr = dst_node
            .create_publisher_raw(
                b.to.topic.as_str(),
                b.type_name.as_str(),
                b.type_hash.as_str(),
            )
            .map_err(|e| ConfigError::BuildEntity(format!("pub on {}: {e:?}", b.to.node)))?;
        drop(dst_node);

        // `'static` origin needed by `PubSubBridge::new` — leak the
        // backend name string. Config-driven entrypoint is one-shot
        // per process; the leak is O(bridges) and bounded.
        let origin: &'static str = Box::leak(src_rmw.to_string().into_boxed_str());
        bridges.push(Box::new(PubSubBridge::new(sub, pubr, origin)));
    }

    // Spin loop: drive each bridge once per executor tick. The
    // executor's own `spin_blocking` would only drain dispatched
    // callbacks; bridges live outside the callback registry by
    // design (they own their handles), so the loop here is explicit.
    use std::time::Duration;
    loop {
        exec.spin_once(Duration::from_millis(10));
        for b in bridges.iter_mut() {
            // Forward every queued sample. Errors short-circuit the
            // loop body but not the whole runtime — a single backend
            // hiccup should not kill the bridge daemon.
            let _ = b.pump();
        }
    }
}

fn node_rmw<'a>(nodes: &'a [NodeCfg], name: &str) -> Result<&'a str, ConfigError> {
    nodes
        .iter()
        .find(|n| n.name == name)
        .map(|n| n.rmw.as_str())
        .ok_or_else(|| ConfigError::UnknownNode(name.into()))
}

/// Trait-object façade so [`run_from_config`] can store bridges of
/// different `RX_BUF` / `TX_BUF` generic instantiations in one Vec.
trait PumpableBridge {
    fn pump(&mut self) -> Result<usize, nros_node::NodeError>;
}

impl<const RX: usize, const TX: usize> PumpableBridge for PubSubBridge<RX, TX> {
    fn pump(&mut self) -> Result<usize, nros_node::NodeError> {
        PubSubBridge::pump(self)
    }
}
