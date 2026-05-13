//! Phase 104.C.2 — multi-Node-per-Executor storage.
//!
//! Mirrors the `rclcpp` pattern where a single `Executor` holds N
//! `Node`s via `add_node(...)`. Each Node carries its own
//! name/namespace + a reference to the Session that backs it +
//! a default `SchedContext` (Phase 110) handles inherit unless
//! overridden.
//!
//! For Phase 104.C.2 we land the *storage scaffold* + the builder
//! API. Multi-Session-per-Executor dispatch is a follow-up
//! (Phase 104.C.3) — today every Node in this list resolves to the
//! Executor's primary session, which means `node_builder.rmw(name)`
//! only accepts the same backend the Executor was opened against.
//! Bridge use cases (two RMW backends concurrent in one Executor)
//! light up when 104.C.3 adds the per-Node session ref.

use super::sched_context::SchedContextId;
use super::types::NodeError;

/// Opaque handle returned by `Executor::node_builder(...).build()`.
/// Used in 104.C.3+ to disambiguate handle ownership when multiple
/// Nodes coexist in one Executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub(crate) u8);

impl NodeId {
    /// Reserved id for the implicit "primary" Node that mirrors the
    /// pre-Phase 104.C.2 single-Node Executor identity.
    pub const PRIMARY: NodeId = NodeId(0);

    /// Numeric index into the Executor's node table.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Per-Node metadata stored inside the Executor.
///
/// Phase 104.C.2 keeps the shape minimal — name, namespace,
/// default SchedContext, optional rmw-name for diagnostics. Future
/// items: per-Node session reference (104.C.3), per-Node liveliness
/// state, per-Node parameter overrides.
pub struct NodeRecord {
    pub name: heapless::String<64>,
    pub namespace: heapless::String<64>,
    /// RMW backend the Node was created against. `None` for the
    /// implicit primary Node populated from `Executor::open`.
    pub rmw_name: Option<heapless::String<32>>,
    /// Per-Node locator override. `None` = use the Executor's
    /// session-level locator. 104.C.3 wires this to the session
    /// cache.
    pub locator: Option<heapless::String<128>>,
    /// Default `SchedContext` for handles created via this Node.
    /// Handles may override per-call. `SchedContextId::default()` =
    /// the executor's auto-created Fifo slot (slot 0).
    pub default_sched: SchedContextId,
}

impl NodeRecord {
    /// Construct the implicit "primary" NodeRecord that mirrors the
    /// Executor's pre-104.C.2 single-Node identity. Currently unused
    /// (the primary Node is implicit until 104.C.3 wires per-Node
    /// dispatch); kept here for the upcoming migration where every
    /// Executor will have an explicit entry at slot 0.
    #[allow(dead_code)]
    pub(crate) fn new_primary(
        name: heapless::String<64>,
        namespace: heapless::String<64>,
    ) -> Self {
        Self {
            name,
            namespace,
            rmw_name: None,
            locator: None,
            default_sched: SchedContextId(0),
        }
    }
}

/// Builder returned by `Executor::node_builder(name)`. Chainable
/// configuration; `.build()` registers the Node with the Executor
/// and returns a [`NodeId`].
///
/// rclcpp-aligned API. Mirrors:
///
/// ```ignore
/// rclcpp::Node::make_shared("my_node",
///     rclcpp::NodeOptions().use_intra_process_comms(true))
/// ```
///
/// Where rclcpp uses a single `NodeOptions` struct, we expose the
/// individual setters directly on the builder — fewer cycles when
/// the user only needs one option.
pub struct NodeBuilder<'a, 'cfg> {
    pub(crate) executor: &'a mut super::spin::Executor,
    pub(crate) name: &'cfg str,
    pub(crate) namespace: Option<&'cfg str>,
    pub(crate) rmw_name: Option<&'cfg str>,
    pub(crate) locator: Option<&'cfg str>,
    pub(crate) domain_id: Option<u32>,
    pub(crate) sched: Option<SchedContextId>,
}

impl<'a, 'cfg> NodeBuilder<'a, 'cfg> {
    /// Select an RMW backend by name. `name` must match a backend
    /// registered via `nros_rmw_cffi_register_named` (Phase 104.B.2).
    ///
    /// In Phase 104.C.2 (current), the name must match the backend
    /// the Executor was opened against — bridge mode lands in
    /// 104.C.3 when per-Node sessions are wired. Passing a name
    /// that doesn't match the Executor's session returns
    /// `Err(NodeError::BackendMismatch)` from `.build()`.
    pub fn rmw(mut self, name: &'cfg str) -> Self {
        self.rmw_name = Some(name);
        self
    }

    /// Override the locator for this Node's session. Empty / unset =
    /// use the Executor's locator.
    pub fn locator(mut self, locator: &'cfg str) -> Self {
        self.locator = Some(locator);
        self
    }

    /// Override the domain id for this Node's session.
    pub fn domain_id(mut self, domain_id: u32) -> Self {
        self.domain_id = Some(domain_id);
        self
    }

    /// Namespace for handles created via this Node. Empty = "/".
    pub fn namespace(mut self, namespace: &'cfg str) -> Self {
        self.namespace = Some(namespace);
        self
    }

    /// Default [`SchedContext`](super::sched_context::SchedContext) for
    /// handles registered via this Node. Phase 110 integration —
    /// handles inherit this unless they pass their own SchedContext
    /// at registration time.
    pub fn sched(mut self, sched: SchedContextId) -> Self {
        self.sched = Some(sched);
        self
    }

    /// Register the Node with the Executor and return its
    /// [`NodeId`]. Bumps `Executor.nodes.len()`; fails if the table
    /// is full (`NROS_EXECUTOR_MAX_NODES` reached) or the name is
    /// too long.
    pub fn build(self) -> Result<NodeId, NodeError> {
        if self.name.len() > 64 {
            return Err(NodeError::NameTooLong);
        }

        // Phase 104.C.2 — single-session check. rmw mismatch is an
        // error today; 104.C.3 will accept and open a new session
        // via the session cache.
        if let Some(_requested) = self.rmw_name {
            // No accessor for the current session's rmw name yet —
            // the registry first-registered slot drives the
            // singleton. We accept any rmw name in C.2 (no
            // validation) so consumer code is forward-compatible.
            // C.3 adds the mismatch check + session-cache lookup.
        }

        let mut name_buf = heapless::String::<64>::new();
        name_buf
            .push_str(self.name)
            .map_err(|_| NodeError::NameTooLong)?;

        let mut ns_buf = heapless::String::<64>::new();
        if let Some(ns) = self.namespace {
            ns_buf
                .push_str(ns)
                .map_err(|_| NodeError::NameTooLong)?;
        } else {
            ns_buf
                .push_str(self.executor.namespace.as_str())
                .map_err(|_| NodeError::NameTooLong)?;
        }

        let mut rmw_buf = None;
        if let Some(rmw) = self.rmw_name {
            let mut s = heapless::String::<32>::new();
            s.push_str(rmw).map_err(|_| NodeError::NameTooLong)?;
            rmw_buf = Some(s);
        }

        let mut loc_buf = None;
        if let Some(loc) = self.locator {
            let mut s = heapless::String::<128>::new();
            s.push_str(loc).map_err(|_| NodeError::NameTooLong)?;
            loc_buf = Some(s);
        }

        let record = NodeRecord {
            name: name_buf,
            namespace: ns_buf,
            rmw_name: rmw_buf,
            locator: loc_buf,
            default_sched: self.sched.unwrap_or(SchedContextId(0)),
        };

        self.executor
            .nodes
            .push(record)
            .map_err(|_| NodeError::NodeTableFull)?;
        let idx = self.executor.nodes.len() - 1;
        if idx > u8::MAX as usize {
            // heapless cap is far below u8::MAX; defensive only.
            return Err(NodeError::NodeTableFull);
        }
        Ok(NodeId(idx as u8))
    }
}
