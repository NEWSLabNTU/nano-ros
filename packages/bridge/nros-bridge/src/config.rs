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
    /// phase-267 W-B1 — ROS-form type name (`std_msgs/msg/Int32`) used to stage
    /// the Cyclone descriptor. `type` above is the DDS-mangled wire form
    /// (`std_msgs::msg::dds_::Int32_`) that pub/sub creation expects; the C
    /// descriptor builder mangles this ROS form into the same wire name and
    /// dual-keys the registry, so `find_descriptor(mangled)` matches. Empty
    /// when the dest backend needs no staged descriptor (e.g. zenoh-only).
    #[serde(default)]
    ros_type: String,
    /// phase-267 W-B1 — flat field schema of the forwarded message, emitted by
    /// `nros sync` from the `.msg`. Drives runtime descriptor staging via
    /// [`nros_rmw::register_type_descriptor`]. Offsets are NOT carried (they
    /// are target-/compile-time only via `offset_of!`); the bridge computes a
    /// self-consistent C packing at startup, which is all the raw-forward path
    /// needs (it round-trips through the descriptor's own `m_size` buffer).
    #[serde(default)]
    fields: Vec<FieldCfg>,
}

/// One `{ name, type }` entry of a [`BridgeCfg::fields`] schema. `type` is the
/// `.msg` primitive name (`int32`, `float64`, `string`, …).
#[derive(serde::Deserialize, Debug, Clone)]
struct FieldCfg {
    name: String,
    #[serde(rename = "type")]
    ty: String,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct BridgeEndpointCfg {
    node: String,
    topic: String,
}

/// Map a `.msg` primitive type name to a [`FieldType`]. Only the leaf types a
/// flat schema can carry are supported; nested / array / sequence fields are
/// rejected (the data-driven bridge demo forwards flat messages — richer
/// schemas need the typed `register::<M>` path). phase-267 W-B1.
fn field_type_from_str(s: &str) -> Result<nros_serdes::schema::FieldType, ConfigError> {
    use nros_serdes::schema::FieldType as F;
    Ok(match s {
        "bool" | "boolean" => F::Bool,
        "uint8" | "octet" | "byte" | "char" => F::Uint8,
        "int8" => F::Int8,
        "uint16" => F::Uint16,
        "int16" => F::Int16,
        "uint32" => F::Uint32,
        "int32" => F::Int32,
        "uint64" => F::Uint64,
        "int64" => F::Int64,
        "float32" | "float" => F::Float32,
        "float64" | "double" => F::Float64,
        "string" => F::String,
        "wstring" => F::WString,
        other => {
            return Err(ConfigError::Parse(format!(
                "unsupported bridge field type {other:?} (flat primitive/string only)"
            )));
        }
    })
}

/// Byte `(size, align)` of a leaf [`FieldType`] in the synthesised descriptor
/// struct. Mirrors `dynamic_type_builder.cpp::primitive_size_align`; strings are
/// pointer-sized. Only needs to be self-consistent — the raw-forward path
/// round-trips through the descriptor's own `m_size` buffer, never a host
/// struct, so these offsets need not match any Rust `offset_of!`. phase-267 W-B1.
fn leaf_size_align(ty: &nros_serdes::schema::FieldType) -> (usize, usize) {
    use nros_serdes::schema::FieldType as F;
    match ty {
        F::Bool | F::Uint8 | F::Int8 => (1, 1),
        F::Uint16 | F::Int16 => (2, 2),
        F::Uint32 | F::Int32 | F::Float32 => (4, 4),
        F::Uint64 | F::Int64 | F::Float64 => (8, 8),
        // string / wstring carry a `char*` in the synthesised struct.
        _ => (
            core::mem::size_of::<*const u8>(),
            core::mem::align_of::<*const u8>(),
        ),
    }
}

/// Build a `'static` [`Field`] slice from a bridge `fields` schema, computing a
/// self-consistent C packing for the offsets. Field names are NUL-terminated
/// and leaked (the C descriptor borrows them for the lifetime of the process —
/// the bridge runs forever, so the leak is bounded + intentional). phase-267 W-B1.
fn build_static_fields(
    fields: &[FieldCfg],
) -> Result<&'static [nros_serdes::schema::Field], ConfigError> {
    use nros_serdes::schema::Field;
    let mut out: Vec<Field> = Vec::with_capacity(fields.len());
    let mut running = 0usize;
    for f in fields {
        let ty = field_type_from_str(&f.ty)?;
        let (size, align) = leaf_size_align(&ty);
        let offset = running.div_ceil(align) * align;
        running = offset + size;
        let name: &'static str = Box::leak(format!("{}\0", f.name).into_boxed_str());
        out.push(Field { name, ty, offset });
    }
    Ok(Box::leak(out.into_boxed_slice()))
}

/// Stage the Cyclone descriptor for a bridge whose dest needs one, before the
/// raw publisher is created (`publisher_create` → `find_descriptor`). No-op when
/// the bridge carries no `fields` (e.g. zenoh→zenoh). On a non-Cyclone build the
/// registrar is absent and `register_type_descriptor` is itself a no-op.
/// phase-267 W-B1.
fn stage_descriptor(b: &BridgeCfg) -> Result<(), ConfigError> {
    if b.fields.is_empty() {
        return Ok(());
    }
    if b.ros_type.is_empty() {
        return Err(ConfigError::Parse(format!(
            "bridge for {:?} carries a field schema but no `ros_type` to stage it under",
            b.type_name
        )));
    }
    let fields = build_static_fields(&b.fields)?;
    let reg_name: &'static str = Box::leak(format!("{}\0", b.ros_type).into_boxed_str());
    nros_rmw::register_type_descriptor(reg_name, fields)
        .map_err(|e| ConfigError::BuildEntity(format!("stage descriptor {:?}: {e:?}", b.ros_type)))
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
    let mut cfg: ConfigFile =
        toml::from_str(raw).map_err(|e| ConfigError::Parse(format!("{e}")))?;
    apply_node_env_overrides(&mut cfg.node);

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
            .create_node_on_with_domain(
                n.name.as_str(),
                n.rmw.as_str(),
                Some(n.domain_id),
                loc_opt(&n.locator),
            )
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
        let src_domain = node_domain(&cfg.node, &b.from.node)?;
        let dst_domain = node_domain(&cfg.node, &b.to.node)?;
        let src_locator = node_locator(&cfg.node, &b.from.node);
        let dst_locator = node_locator(&cfg.node, &b.to.node);

        let mut src_node = exec
            .create_node_on_with_domain(
                b.from.node.as_str(),
                src_rmw,
                Some(src_domain),
                src_locator,
            )
            .map_err(|e| ConfigError::BuildNode(format!("{}: {e:?}", b.from.node)))?;
        let sub = src_node
            .create_subscription_raw(
                b.from.topic.as_str(),
                b.type_name.as_str(),
                b.type_hash.as_str(),
            )
            .map_err(|e| ConfigError::BuildEntity(format!("sub on {}: {e:?}", b.from.node)))?;
        drop(src_node);

        // Stage the dest descriptor BEFORE the raw publisher — `publisher_create`
        // resolves it via `find_descriptor` and fails `PublisherCreationFailed`
        // (issue 0107) otherwise. No-op for backends/types that carry no schema.
        stage_descriptor(b)?;

        let mut dst_node = exec
            .create_node_on_with_domain(b.to.node.as_str(), dst_rmw, Some(dst_domain), dst_locator)
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

/// phase-267 #113 — apply per-node env overrides over the baked config.
///
/// For each `[[node]]` named `<N>`, `NROS_BRIDGE_<N>_LOCATOR` overrides its
/// `locator` and `NROS_BRIDGE_<N>_DOMAIN` its `domain_id` (`<N>` upper-cased,
/// non-alphanumerics → `_`). Empty / unparseable values are ignored (the baked
/// value stands). Lets a deployed bridge be re-pointed at a different router /
/// DDS domain — and a test point it at an ephemeral router + unique domain —
/// without a rebuild. `run_from_config` bakes the config via `include_str!`, so
/// without this the endpoints are compile-time fixed.
fn apply_node_env_overrides(nodes: &mut [NodeCfg]) {
    for n in nodes.iter_mut() {
        let key = env_key(&n.name);
        if let Ok(loc) = std::env::var(format!("NROS_BRIDGE_{key}_LOCATOR"))
            && !loc.is_empty()
        {
            n.locator = loc;
        }
        if let Ok(dom) = std::env::var(format!("NROS_BRIDGE_{key}_DOMAIN"))
            && let Ok(d) = dom.trim().parse::<u32>()
        {
            n.domain_id = d;
        }
    }
}

/// Env-var-safe form of a node name: upper-case, non-alphanumerics → `_`.
fn env_key(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// Configured domain id of a `[[node]]`. An extra RMW session's participant
/// domain follows the node builder's `domain_id`, not the `SessionSpec`'s, so
/// the bridge must thread it explicitly (phase-267 issue 0109).
/// `Some(locator)` when non-empty, else `None` (rmw-default). An agent-based
/// backend (xrce) MUST carry a locator; DDS/multicast (cyclonedds) carries none.
fn loc_opt(locator: &str) -> Option<&str> {
    (!locator.is_empty()).then_some(locator)
}

/// Configured locator of a `[[node]]` (the xrce agent addr; empty → `None`).
fn node_locator<'a>(nodes: &'a [NodeCfg], name: &str) -> Option<&'a str> {
    nodes
        .iter()
        .find(|n| n.name == name)
        .and_then(|n| loc_opt(&n.locator))
}

fn node_domain(nodes: &[NodeCfg], name: &str) -> Result<u32, ConfigError> {
    nodes
        .iter()
        .find(|n| n.name == name)
        .map(|n| n.domain_id)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fc(name: &str, ty: &str) -> FieldCfg {
        FieldCfg {
            name: name.into(),
            ty: ty.into(),
        }
    }

    #[test]
    fn build_static_fields_packs_self_consistent_offsets() {
        // phase-267 W-B1 — int8 then int32: int8 at 0, int32 aligned to 4. The
        // raw-forward path only needs self-consistent C packing (it round-trips
        // through the descriptor's own buffer), not the host `offset_of!`.
        let fields = build_static_fields(&[fc("flag", "int8"), fc("count", "int32")]).unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].offset, 0);
        assert_eq!(fields[1].offset, 4);
        // Names are NUL-terminated for the C descriptor.
        assert_eq!(fields[0].name, "flag\0");
        assert_eq!(fields[1].name, "count\0");
    }

    #[test]
    fn field_type_from_str_rejects_unknown() {
        assert!(field_type_from_str("int32").is_ok());
        assert!(field_type_from_str("not_a_type").is_err());
    }

    #[test]
    fn env_key_uppercases_and_sanitizes() {
        assert_eq!(env_key("s0"), "S0");
        assert_eq!(env_key("field-control.1"), "FIELD_CONTROL_1");
    }

    #[test]
    fn apply_node_env_overrides_replaces_locator_and_domain() {
        // Unique var names so the test is parallel-safe (env is process-global).
        unsafe {
            std::env::set_var("NROS_BRIDGE_ENVT_LOCATOR", "tcp/10.0.0.9:7448");
            std::env::set_var("NROS_BRIDGE_ENVT_DOMAIN", "42");
        }
        let mut nodes = vec![NodeCfg {
            name: "envt".into(),
            rmw: "zenoh".into(),
            locator: "tcp/127.0.0.1:7447".into(),
            domain_id: 0,
            namespace: String::new(),
        }];
        apply_node_env_overrides(&mut nodes);
        assert_eq!(nodes[0].locator, "tcp/10.0.0.9:7448");
        assert_eq!(nodes[0].domain_id, 42);
        unsafe {
            std::env::remove_var("NROS_BRIDGE_ENVT_LOCATOR");
            std::env::remove_var("NROS_BRIDGE_ENVT_DOMAIN");
        }
    }

    #[test]
    fn stage_descriptor_errs_when_fields_present_without_ros_type() {
        let b = BridgeCfg {
            type_name: "std_msgs::msg::dds_::Int32_".into(),
            type_hash: String::new(),
            from: BridgeEndpointCfg {
                node: "s0".into(),
                topic: "/c".into(),
            },
            to: BridgeEndpointCfg {
                node: "s1".into(),
                topic: "/c".into(),
            },
            ros_type: String::new(),
            fields: vec![fc("data", "int32")],
        };
        assert!(stage_descriptor(&b).is_err());
    }
}
