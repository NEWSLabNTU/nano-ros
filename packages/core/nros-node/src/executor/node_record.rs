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

use super::{sched_context::SchedContextId, types::NodeError};

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

    /// Phase 104.C.8.b / C.9.b — build a `NodeId` from a raw `u8` for
    /// FFI consumers that store the index in their own struct
    /// (`nros_node_t.node_id`, `nros_cpp_node_t.node_id`). The value
    /// is not validated against the executor's `nodes` table — the
    /// caller is responsible for only constructing ids that
    /// `node_builder(...).build()` previously returned. Out-of-range
    /// ids fail loudly at the next `with_node` / `_on(...)` call.
    pub const fn from_raw(raw: u8) -> NodeId {
        NodeId(raw)
    }

    /// Raw u8 form for FFI persistence.
    pub const fn raw(self) -> u8 {
        self.0
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
    /// session-level locator.
    pub locator: Option<heapless::String<128>>,
    /// Default `SchedContext` for handles created via this Node.
    /// Handles may override per-call. `SchedContextId::default()` =
    /// the executor's auto-created Fifo slot (slot 0).
    pub default_sched: SchedContextId,
    /// Phase 104.C.3 — session-slot index. `0` resolves to the
    /// Executor's primary `session` field; `N >= 1` resolves to
    /// `extra_sessions[N-1]`. Each Node may bind to a different
    /// session, enabling multi-RMW bridges in one Executor.
    pub session_idx: u8,
}

impl NodeRecord {
    /// Construct the implicit "primary" NodeRecord that mirrors the
    /// Executor's pre-104.C.2 single-Node identity. Currently unused
    /// (the primary Node is implicit until 104.C.3 wires per-Node
    /// dispatch); kept here for the upcoming migration where every
    /// Executor will have an explicit entry at slot 0.
    #[allow(dead_code)]
    pub(crate) fn new_primary(name: heapless::String<64>, namespace: heapless::String<64>) -> Self {
        Self {
            name,
            namespace,
            rmw_name: None,
            locator: None,
            default_sched: SchedContextId(0),
            session_idx: 0,
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
pub struct NodeBuilder<'a, 'cfg, 's> {
    pub(crate) executor: &'a mut super::spin::Executor<'s>,
    pub(crate) name: &'cfg str,
    pub(crate) namespace: Option<&'cfg str>,
    pub(crate) rmw_name: Option<&'cfg str>,
    pub(crate) locator: Option<&'cfg str>,
    pub(crate) domain_id: Option<u32>,
    pub(crate) sched: Option<SchedContextId>,
    /// Phase 172.K.5 — explicit session slot (index into the sessions opened
    /// by `open_multi`: 0 = primary, N = `extra_sessions[N-1]`). When set,
    /// `build()` binds the Node directly to this session and **bypasses** the
    /// rmw-based `resolve_session_slot` — the planner/generator already knows
    /// which `SESSION_SPECS` slot each node belongs to (e.g. its domain group),
    /// so no rmw/domain inference is needed. `None` ⇒ the legacy rmw-resolved
    /// slot.
    pub(crate) session_idx: Option<u8>,
}

impl<'a, 'cfg, 's> NodeBuilder<'a, 'cfg, 's> {
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

    /// Phase 172.K.5 — bind this Node to an explicit session slot (index into
    /// the sessions opened by [`Executor::open_multi`]: `0` = primary,
    /// `N` = `extra_sessions[N-1]`). Bypasses the rmw-based session resolution
    /// — the caller (generated multi-domain wiring) already knows the slot.
    pub fn session_idx(mut self, idx: u8) -> Self {
        self.session_idx = Some(idx);
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

    /// Phase 104.C.3 — pick a session slot for the Node being
    /// built. Returns `0` for the primary session (no rmw override
    /// or rmw matches existing) and `N >= 1` for an extra session
    /// just opened via `CffiRmw::open_with_rmw`.
    #[cfg(feature = "rmw-cffi")]
    fn resolve_session_slot(&mut self) -> Result<u8, NodeError> {
        let Some(rmw) = self.rmw_name else {
            return Ok(0);
        };

        // Phase 156 — check primary FIRST. Executor::open* records
        // `primary_rmw_name` + `primary_locator` so we can detect
        // when a `.rmw(name)` matches the primary session and
        // return slot 0 instead of opening a SECOND backend
        // session against the same singleton (which zenoh-pico's
        // global g_session forbids). Locator-None means "inherit
        // primary"; locator-Some must match primary's exactly.
        // Empty `primary_rmw_name` → constructed via
        // `from_session(_ptr)` without `open*` recording — fall
        // through to extras cache + new-session path.
        if !self.executor.primary_rmw_name.is_empty()
            && self.executor.primary_rmw_name.as_str() == rmw
        {
            let locator_matches = match self.locator {
                None => true,
                Some(loc) => self.executor.primary_locator.as_str() == loc,
            };
            if locator_matches {
                return Ok(0);
            }
        }

        // Reuse an extra session if one already opened against the
        // same rmw + locator. Slot 0 (primary) handled by the
        // primary-identity check above.
        for (i, sess) in self.executor.extra_sessions.iter().enumerate() {
            let _ = sess;
            // Phase 104.C.3 doesn't yet store rmw-name per session;
            // dedupe by NodeRecord's stored rmw_name + locator.
            if let Some(prev) = self.executor.nodes.iter().find(|n| {
                n.session_idx as usize == i + 1
                    && n.rmw_name.as_deref() == Some(rmw)
                    && n.locator.as_deref() == self.locator
            }) {
                let _ = prev;
                return Ok((i + 1) as u8);
            }
        }

        // First Node naming this rmw → open a new session.
        let mode = nros_rmw::SessionMode::Client;
        let locator = self.locator.unwrap_or("");
        let domain_id = self.domain_id.unwrap_or(0);
        let cfg = nros_rmw::RmwConfig {
            locator,
            mode,
            domain_id,
            node_name: self.name,
            namespace: self.namespace.unwrap_or(""),
            properties: &[],
        };
        let session = nros_rmw_cffi::CffiRmw::open_with_rmw(rmw, &cfg)
            .map_err(crate::executor::types::NodeError::Transport)?;
        self.executor
            .extra_sessions
            .push(session)
            .map_err(|_| NodeError::NodeTableFull)?;
        let idx = self.executor.extra_sessions.len();
        if idx > u8::MAX as usize {
            return Err(NodeError::NodeTableFull);
        }
        // Phase 104.C.6.b — install the shared wake flag on the
        // freshly opened extra session so its backend notifications
        // can short-circuit `spin_once`.
        #[cfg(feature = "std")]
        self.executor.install_wake_signal_on_extra(idx - 1);
        Ok(idx as u8)
    }

    #[cfg(not(feature = "rmw-cffi"))]
    fn resolve_session_slot(&mut self) -> Result<u8, NodeError> {
        // Without `rmw-cffi`, only the primary session exists. An
        // rmw-name override is meaningless; treat as the primary.
        Ok(0)
    }

    /// Register the Node with the Executor and return its
    /// [`NodeId`]. Bumps `Executor.nodes.len()`; fails if the table
    /// is full (`NROS_EXECUTOR_MAX_NODES` reached) or the name is
    /// too long.
    pub fn build(mut self) -> Result<NodeId, NodeError> {
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
            ns_buf.push_str(ns).map_err(|_| NodeError::NameTooLong)?;
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

        // Phase 172.K.5 — an explicit `.session_idx(n)` binds the Node to a
        // pre-opened `open_multi` session directly (validated against the
        // opened set), bypassing rmw resolution. Otherwise (Phase 104.C.3)
        // resolve by rmw: slot 0 for no/primary-matching rmw, else open/reuse
        // an extra session.
        let session_idx = match self.session_idx {
            Some(idx) => {
                if idx as usize > self.executor.extra_sessions.len() {
                    return Err(NodeError::NodeTableFull);
                }
                idx
            }
            None => self.resolve_session_slot()?,
        };

        // Phase 272 (RFC-0047) — resolve default_sched with precedence:
        //   explicit .sched()  >  table lookup  >  SchedContextId(0)
        //
        // The lookup borrows `self.executor` immutably and returns a Copy
        // value, so the borrow ends before the mutable `nodes.push` below.
        let default_sched = match self.sched {
            Some(id) => id,
            None => self
                .executor
                .lookup_node_sched(name_buf.as_str(), ns_buf.as_str())
                .unwrap_or(SchedContextId(0)),
        };

        let record = NodeRecord {
            name: name_buf,
            namespace: ns_buf,
            rmw_name: rmw_buf,
            locator: loc_buf,
            default_sched,
            session_idx,
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
