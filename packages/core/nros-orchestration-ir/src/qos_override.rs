//! Issue 0303 — lowering `qos_overrides.<topic>.<role>.<policy>` parameters
//! into the primitive `(topic, role, policy, value)` codes every language bakes.
//!
//! ROS 2 declares per-topic QoS through ordinary parameters:
//!
//! ```text
//! qos_overrides./chatter.publisher.reliability = best_effort
//! qos_overrides./chatter.subscription.depth    = 10
//! ```
//!
//! This module is where those strings become codes, for BOTH producers — the
//! `nros` CLI's C/C++ entry emitters and the `nros::main!` proc-macro. They
//! used to carry a copy each; the copies disagreed about nothing yet, but the
//! decoder they feed had already been forgotten in two of four places (see
//! `nros_rmw::decode_qos_override`), and this is the same shape one level up.
//! Same rationale as this crate already owning tier resolution: whatever the
//! macro and the CLI must agree on lives here.
//!
//! **Errors are values, not silence.** Before this module both producers
//! `filter_map`ed an unrecognised role or policy away, so
//! `qos_overrides./t.pub.reliability` (`pub` for `publisher`) or a policy the
//! bake does not model produced no override and no diagnostic — the image ran
//! different delivery semantics than the model declared. Every rejection here
//! is a typed error the caller must handle.

use core::fmt;

use nros_rmw::{qos_override_policy, qos_override_role};

// Re-exported so a consumer of the lowering can name the codes it produced
// without depping `nros-rmw` itself (the CLI does not).
pub use nros_rmw::{qos_override_policy as policy, qos_override_role as role};

/// The parameter-name prefix that marks a QoS override.
pub const QOS_OVERRIDE_PREFIX: &str = "qos_overrides.";

/// One lowered override: `(topic, role, policy, value)`, matching
/// [`nros_rmw::QoSOverrideCode`] but owning its topic (the producers turn it
/// into a baked literal).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LoweredOverride {
    /// Resolved topic, e.g. `"/chatter"`.
    pub topic: String,
    /// [`nros_rmw::qos_override_role`] code.
    pub role: u8,
    /// [`nros_rmw::qos_override_policy`] code.
    pub policy: u8,
    /// Policy-specific value.
    pub value: u32,
}

/// Why a `qos_overrides.*` parameter could not be lowered.
///
/// Every variant names the offending key AND the accepted spellings: a QoS
/// mistake is invisible at runtime, so the build message is the only place a
/// user can learn what they typed wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QoSOverrideError {
    /// The key has the prefix but not the `<topic>.<role>.<policy>` shape.
    Malformed { key: String },
    /// The role segment is not `publisher` or `subscription`.
    UnknownRole { key: String, role: String },
    /// The policy segment is not one this build models.
    UnknownPolicy { key: String, policy: String },
    /// The value does not parse for its policy.
    BadValue {
        key: String,
        policy: String,
        value: String,
        expected: &'static str,
    },
}

impl fmt::Display for QoSOverrideError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QoSOverrideError::Malformed { key } => write!(
                f,
                "parameter `{key}` looks like a QoS override but is not \
                 `qos_overrides.<topic>.<role>.<policy>`"
            ),
            QoSOverrideError::UnknownRole { key, role } => write!(
                f,
                "parameter `{key}`: unknown QoS override role `{role}` \
                 (expected `publisher` or `subscription`)"
            ),
            QoSOverrideError::UnknownPolicy { key, policy } => write!(
                f,
                "parameter `{key}`: unknown QoS override policy `{policy}` (expected one of: \
                 {POLICY_NAMES})"
            ),
            QoSOverrideError::BadValue {
                key,
                policy,
                value,
                expected,
            } => write!(
                f,
                "parameter `{key}`: `{value}` is not a valid `{policy}` value (expected {expected})"
            ),
        }
    }
}

impl std::error::Error for QoSOverrideError {}

/// The policy spellings this build models, for diagnostics.
const POLICY_NAMES: &str = "reliability, durability, history, depth, deadline, lifespan, \
                            liveliness, liveliness_lease_duration";

/// Every policy [`lower`] accepts, and whether it bounds CAPACITY.
///
/// phase-454 W7 (RFC-0100 D8). The second flag is the whole of this wave's
/// scope decision, so it is a field rather than a comment: `reliability`,
/// `durability`, `history` and `depth` decide how many bytes an image must
/// reserve, and the launch CONTRACT states the same four per endpoint
/// (`QosDecl`, read since W3). Those two statements have to agree.
/// `deadline`, `lifespan` and the two liveliness policies bound OCCUPANCY or
/// LIVENESS of a queue whose size is already decided, so they size nothing --
/// but the contract states them too (`deadline`, `lifespan`, `lease_duration`,
/// `liveliness`), and since phase-462 W5 [`contract_statement`] reads all
/// eight, so a parameter that disagrees with its contract is refused either way.
///
/// A table and not a `match` because it is read by three things that must not
/// drift: [`contract_statement`] classifies a policy against it, `POLICY_NAMES`
/// enumerates it for the diagnostic, and the tests assert that what [`lower`]
/// accepts is exactly what this lists — a new policy that reaches `lower` and
/// not this table is a policy nobody decided the sizing question for.
pub const MODELLED_POLICIES: [(&str, bool); 8] = [
    ("reliability", true),
    ("durability", true),
    ("history", true),
    ("depth", true),
    ("deadline", false),
    ("lifespan", false),
    ("liveliness", false),
    ("liveliness_lease_duration", false),
];

// ---------------------------------------------------------------------------
// The VALUE vocabulary -- phase-454 W3.
// ---------------------------------------------------------------------------
//
// `best_effort` / `transient_local` / `keep_all` are ROS 2's own spellings, and
// until W3 they were read in exactly one place: [`lower`] below, against
// `qos_overrides.*` parameters. The launch CONTRACT states the same three
// policies in the same three vocabularies (`QosDecl::{reliability, durability,
// history}` in the pinned `ros-launch-manifest`), and nano-ros dropped them at
// `depth_of` -- issue 1256.
//
// W3 makes them travel, which means a SECOND reader of the same spellings. A
// second reader is a second vocabulary unless it is the same function, and this
// module's own header says what that costs: the four copies of the DECODER
// drifted, and "whatever the macro and the CLI must agree on lives here".
//
// So the parse is here, once, and it returns `nros-rmw`'s enums -- the types
// the runtime already folds an override into (`QoSProfile::apply_override_value`)
// -- rather than a fresh set. Re-exported below so a caller need not dep
// `nros-rmw`, exactly as the role/policy codes already are.
//
// `SystemDefault` is deliberately NOT a spelling either surface accepts. It is
// a real upstream policy value, but "the author wrote `system_default`" and
// "the author said nothing" are different claims that every consumer here would
// have to distinguish, and no producer in this tree states it. Absence is the
// spelling for absence.

// Re-exported for the same reason the code constants are: a consumer of the
// vocabulary should not have to dep `nros-rmw` to name what it parsed.
pub use nros_rmw::{QoSDurabilityPolicy, QoSHistoryPolicy, QoSReliabilityPolicy};

/// The accepted `reliability` spellings, for a diagnostic.
pub const RELIABILITY_VALUES: &str = "`best_effort` or `reliable`";
/// The accepted `durability` spellings, for a diagnostic.
pub const DURABILITY_VALUES: &str = "`volatile` or `transient_local`";
/// The accepted `history` spellings, for a diagnostic.
pub const HISTORY_VALUES: &str = "`keep_last` or `keep_all`";

/// Parse a `reliability` value. `None` for a spelling this build does not
/// model -- an ERROR for every caller, never a skip.
pub fn parse_reliability(value: &str) -> Option<QoSReliabilityPolicy> {
    match value.trim() {
        "best_effort" => Some(QoSReliabilityPolicy::BestEffort),
        "reliable" => Some(QoSReliabilityPolicy::Reliable),
        _ => None,
    }
}

/// Parse a `durability` value. `None` for a spelling this build does not model.
pub fn parse_durability(value: &str) -> Option<QoSDurabilityPolicy> {
    match value.trim() {
        "volatile" => Some(QoSDurabilityPolicy::Volatile),
        "transient_local" => Some(QoSDurabilityPolicy::TransientLocal),
        _ => None,
    }
}

/// Parse a `history` value. `None` for a spelling this build does not model.
pub fn parse_history(value: &str) -> Option<QoSHistoryPolicy> {
    match value.trim() {
        "keep_last" => Some(QoSHistoryPolicy::KeepLast),
        "keep_all" => Some(QoSHistoryPolicy::KeepAll),
        _ => None,
    }
}

/// The canonical spelling of a parsed `reliability`, for rendering it back out.
///
/// `SystemDefault` renders as `system_default` even though no surface PARSES
/// it: a renderer that had to return `Option` would make every call site
/// decide what an unrenderable policy means, and this way the round trip is
/// total in one direction and documented in the other.
pub fn reliability_spelling(p: QoSReliabilityPolicy) -> &'static str {
    match p {
        QoSReliabilityPolicy::SystemDefault => "system_default",
        QoSReliabilityPolicy::Reliable => "reliable",
        QoSReliabilityPolicy::BestEffort => "best_effort",
        // A read-back sentinel, not a policy a launch file can state. It has
        // no `qos_overrides.*` spelling upstream either — there is nothing to
        // render it INTO — so it renders as what it is.
        QoSReliabilityPolicy::Unknown => "unknown",
    }
}

/// The canonical spelling of a parsed `durability`. See [`reliability_spelling`].
pub fn durability_spelling(p: QoSDurabilityPolicy) -> &'static str {
    match p {
        QoSDurabilityPolicy::SystemDefault => "system_default",
        QoSDurabilityPolicy::Volatile => "volatile",
        QoSDurabilityPolicy::TransientLocal => "transient_local",
        // See `reliability_spelling`.
        QoSDurabilityPolicy::Unknown => "unknown",
    }
}

/// The canonical spelling of a parsed `history`. See [`reliability_spelling`].
pub fn history_spelling(p: QoSHistoryPolicy) -> &'static str {
    match p {
        QoSHistoryPolicy::SystemDefault => "system_default",
        QoSHistoryPolicy::KeepLast => "keep_last",
        QoSHistoryPolicy::KeepAll => "keep_all",
        // See `reliability_spelling`.
        QoSHistoryPolicy::Unknown => "unknown",
    }
}

/// Is this parameter name a QoS override?
pub fn is_qos_override(name: &str) -> bool {
    name.starts_with(QOS_OVERRIDE_PREFIX)
}

/// The canonical spelling of a [`qos_override_role`] code, for a diagnostic.
///
/// The inverse of the role match in `split_key`, and the reason it is a
/// function: phase-454 W7's messages name the role the author WROTE, and a
/// message that respelled it would send someone looking for a key they did not
/// type.
pub fn role_spelling(role: u8) -> &'static str {
    match role {
        qos_override_role::SUBSCRIPTION => "subscription",
        // `PUBLISHER` is 0 and a code this build does not know cannot be
        // produced by `split_key`, which is the only producer.
        _ => "publisher",
    }
}

/// Split `qos_overrides.<topic>.<role>.<policy>` into its three segments.
///
/// `Ok(None)` when `name` carries no prefix. THE one parser of the key shape:
/// [`lower`] and [`contract_statement`] are two readers of the same parameter and
/// this module's own header says what a second spelling costs. The VALUE
/// vocabulary is shared the same way, through `parse_reliability` and its
/// siblings.
fn split_key(name: &str) -> Result<Option<(&str, u8, &str)>, QoSOverrideError> {
    let Some(rest) = name.strip_prefix(QOS_OVERRIDE_PREFIX) else {
        return Ok(None);
    };
    let key = name.to_string();

    // rsplitn(3, '.') → [policy, role, topic]: the topic may itself contain
    // dots, the trailing two segments may not.
    let mut parts = rest.rsplitn(3, '.');
    let (Some(policy_s), Some(role_s), Some(topic)) = (parts.next(), parts.next(), parts.next())
    else {
        return Err(QoSOverrideError::Malformed { key });
    };
    if topic.is_empty() || role_s.is_empty() || policy_s.is_empty() {
        return Err(QoSOverrideError::Malformed { key });
    }

    let role = match role_s {
        "publisher" => qos_override_role::PUBLISHER,
        "subscription" => qos_override_role::SUBSCRIPTION,
        _ => {
            return Err(QoSOverrideError::UnknownRole {
                key,
                role: role_s.to_string(),
            });
        }
    };
    Ok(Some((topic, role, policy_s)))
}

/// Nanoseconds in one millisecond: the wire's resolution (issue 1855).
const NS_PER_MS: u64 = 1_000_000;

/// What a duration policy's value must be (issue 1855), for the diagnostic.
const DURATION_NS: &str = "a duration in NANOSECONDS, rclcpp's unit (500 ms is 500000000): \
                           0 for none, at least 1000000 (1 ms, the wire's resolution), \
                           below 4294967295 ms, or 9223372036854775807 for infinite";

/// A duration policy's parameter value (NANOSECONDS, issue 1855) in the wire's
/// milliseconds, by the runtime's own boundary rule; `None` for a value
/// [`lower`] refuses. ONE conversion, shared with the contract comparison, so
/// "the same duration" means the same thing on both sides.
fn duration_ns_to_wire_ms(v: &str) -> Option<u32> {
    let n = v.parse::<u64>().ok()?;
    if n == i64::MAX as u64 {
        return Some(nros_rmw::DURATION_INFINITE_MS);
    }
    if n > 0 && n < NS_PER_MS {
        return None;
    }
    nros_rmw::duration_to_qos_ms(core::time::Duration::from_nanos(n)).ok()
}

/// A contract duration (milliseconds, as the model carries it) in the wire's
/// milliseconds: the same rule `duration_ns_to_wire_ms` applies to a
/// parameter, so a contract `deadline: 100ms` and a parameter `100000000`
/// compare equal. `None` for a value the wire cannot carry.
#[must_use]
pub fn contract_ms_to_wire_ms(ms: f64) -> Option<u32> {
    if !ms.is_finite() || ms < 0.0 {
        return None;
    }
    nros_rmw::duration_to_qos_ms(core::time::Duration::from_secs_f64(ms / 1_000.0)).ok()
}

/// The liveliness kinds, as both the parameter and the contract spell them.
pub const LIVELINESS_VALUES: &str = "`none`, `automatic`, `manual_by_topic` or `manual_by_node`";

/// Parse a liveliness kind; the one reader of the spelling, for both surfaces.
#[must_use]
pub fn parse_liveliness(value: &str) -> Option<&'static str> {
    ["none", "automatic", "manual_by_topic", "manual_by_node"]
        .into_iter()
        .find(|k| *k == value.trim())
}

/// Lower one `qos_overrides.<topic>.<role>.<policy>` parameter.
///
/// Returns `Ok(None)` when `name` is not a QoS override at all — the caller is
/// walking a mixed parameter list. A name that DOES carry the prefix but is
/// unusable is an `Err`, never a skip.
pub fn lower(name: &str, value: &str) -> Result<Option<LoweredOverride>, QoSOverrideError> {
    let Some((topic, role, policy_s)) = split_key(name)? else {
        return Ok(None);
    };
    let key = name.to_string();

    let v = value.trim();
    let bad = |expected: &'static str| QoSOverrideError::BadValue {
        key: name.to_string(),
        policy: policy_s.to_string(),
        value: v.to_string(),
        expected,
    };
    let ms = |expected| v.parse::<u32>().map_err(|_| bad(expected));
    // Issue 1855 -- the duration policies are spelled in NANOSECONDS, rclcpp's
    // unit (`QosOverridingOptions` declares them as int64 nanoseconds), which
    // is also what play_launch's resolver derives from a contract's
    // `deadline:` / `lease_duration:`. The wire carries milliseconds, so the
    // value is converted here, by the one boundary rule the runtime already
    // uses (`nros_rmw::duration_to_qos_ms`: zero stays "unset", a remainder
    // CEILS, a value at or past the infinite sentinel is refused). rclcpp's
    // own spelling of an infinite duration, `i64::MAX` nanoseconds, maps to
    // the sentinel. A NONZERO value under one millisecond is refused rather
    // than rounded up: it is below the wire's resolution, and it is exactly
    // what a millisecond spelling from before this issue looks like.
    let ns = |expected| duration_ns_to_wire_ms(v).ok_or_else(|| bad(expected));

    let (policy, value) = match policy_s {
        // phase-454 W3 -- the SPELLINGS come from `parse_*` above, which the
        // contract reader also calls; the 0/1 here is this WIRE's own encoding
        // and not the enum's discriminant (`decode_qos_override_value` is the
        // other end of it). Keeping the two apart is deliberate: the wire codes
        // are an ABI, the enum's numbering is not, and phase-376 W5/B2 already
        // renumbered the liveliness enum under a literal that kept compiling.
        // Matching on the VARIANT is what makes a renumbering a compile error
        // here instead of a silent policy swap.
        "reliability" => (
            qos_override_policy::RELIABILITY,
            match parse_reliability(v) {
                Some(QoSReliabilityPolicy::BestEffort) => 0,
                Some(QoSReliabilityPolicy::Reliable) => 1,
                // Unreachable while `parse_reliability` refuses the spelling;
                // written out rather than `_` so adding one is a decision here.
                Some(QoSReliabilityPolicy::SystemDefault | QoSReliabilityPolicy::Unknown)
                | None => {
                    return Err(bad(RELIABILITY_VALUES));
                }
            },
        ),
        "durability" => (
            qos_override_policy::DURABILITY,
            match parse_durability(v) {
                Some(QoSDurabilityPolicy::Volatile) => 0,
                Some(QoSDurabilityPolicy::TransientLocal) => 1,
                Some(QoSDurabilityPolicy::SystemDefault | QoSDurabilityPolicy::Unknown) | None => {
                    return Err(bad(DURABILITY_VALUES));
                }
            },
        ),
        "history" => (
            qos_override_policy::HISTORY,
            match parse_history(v) {
                Some(QoSHistoryPolicy::KeepLast) => 0,
                Some(QoSHistoryPolicy::KeepAll) => 1,
                Some(QoSHistoryPolicy::SystemDefault | QoSHistoryPolicy::Unknown) | None => {
                    return Err(bad(HISTORY_VALUES));
                }
            },
        ),
        "depth" => (qos_override_policy::DEPTH, ms("a non-negative integer")?),
        "deadline" => (qos_override_policy::DEADLINE, ns(DURATION_NS)?),
        "lifespan" => (qos_override_policy::LIFESPAN, ns(DURATION_NS)?),
        "liveliness" => (
            qos_override_policy::LIVELINESS,
            match v {
                // The discriminants of `nros_rmw::QoSLivelinessPolicy`, NAMED
                // rather than written out. Phase 376 W5/B2 renumbered that enum
                // to upstream's ordering (MANUAL_BY_NODE 3 -> 2,
                // MANUAL_BY_TOPIC 2 -> 3) and these literals kept compiling
                // while meaning the other policy. The decoder in
                // `nros_rmw::traits` is the other half of this wire; naming the
                // variant is what keeps the two ends from drifting apart, and
                // the comment claiming they were discriminants was the only
                // thing binding them before.
                "none" => nros_rmw::QoSLivelinessPolicy::None as u32,
                "automatic" => nros_rmw::QoSLivelinessPolicy::Automatic as u32,
                "manual_by_topic" => nros_rmw::QoSLivelinessPolicy::ManualByTopic as u32,
                "manual_by_node" => nros_rmw::QoSLivelinessPolicy::ManualByNode as u32,
                _ => {
                    return Err(bad(LIVELINESS_VALUES));
                }
            },
        ),
        "liveliness_lease_duration" => (qos_override_policy::LIVELINESS_LEASE, ns(DURATION_NS)?),
        _ => {
            return Err(QoSOverrideError::UnknownPolicy {
                key,
                policy: policy_s.to_string(),
            });
        }
    };

    Ok(Some(LoweredOverride {
        topic: topic.to_string(),
        role,
        policy,
        value,
    }))
}

/// What one `qos_overrides.*` parameter states about SIZING, in the same
/// vocabulary the launch contract states it in (phase-454 W7, RFC-0100 D8).
///
/// The four capacity policies of [`MODELLED_POLICIES`] and no others. This is
/// deliberately NOT the wire encoding [`LoweredOverride`] carries: comparing an
/// override against a contract is a comparison of two AUTHORED statements, and
/// the wire's `0`/`1` are an ABI that says nothing about what either author
/// wrote. `reliability_spelling` and its siblings render these back out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QosStatement {
    Reliability(QoSReliabilityPolicy),
    Durability(QoSDurabilityPolicy),
    History(QoSHistoryPolicy),
    Depth(u32),
    /// phase-462 W5 -- the four liveness policies, in the wire's
    /// milliseconds (converted by `duration_ns_to_wire_ms`).
    Deadline(u32),
    Lifespan(u32),
    LivelinessLease(u32),
    /// A kind from [`parse_liveliness`].
    Liveliness(&'static str),
}

impl QosStatement {
    /// The policy's spelling in a parameter key and in a contract `qos:` block
    /// — one name, because it is one fact stated twice.
    pub fn policy(&self) -> &'static str {
        match self {
            QosStatement::Reliability(_) => "reliability",
            QosStatement::Durability(_) => "durability",
            QosStatement::History(_) => "history",
            QosStatement::Depth(_) => "depth",
            QosStatement::Deadline(_) => "deadline",
            QosStatement::Lifespan(_) => "lifespan",
            QosStatement::LivelinessLease(_) => "liveliness_lease_duration",
            QosStatement::Liveliness(_) => "liveliness",
        }
    }

    /// The same policy's key in a contract `qos:` block. One word differs:
    /// the parameter's `liveliness_lease_duration` is the contract's
    /// `lease_duration`.
    pub fn contract_key(&self) -> &'static str {
        match self {
            QosStatement::LivelinessLease(_) => "lease_duration",
            other => other.policy(),
        }
    }

    /// Whether the policy bounds CAPACITY ([`MODELLED_POLICIES`]' second
    /// column): what sizes an arena, as opposed to the liveness of a queue
    /// whose size is decided.
    pub fn is_capacity(&self) -> bool {
        matches!(
            self,
            QosStatement::Reliability(_)
                | QosStatement::Durability(_)
                | QosStatement::History(_)
                | QosStatement::Depth(_)
        )
    }

    /// The VALUE, spelled as an author would write it on either surface.
    pub fn value(&self) -> String {
        match self {
            QosStatement::Reliability(p) => reliability_spelling(*p).to_string(),
            QosStatement::Durability(p) => durability_spelling(*p).to_string(),
            QosStatement::History(p) => history_spelling(*p).to_string(),
            QosStatement::Depth(d) => d.to_string(),
            QosStatement::Deadline(ms)
            | QosStatement::Lifespan(ms)
            | QosStatement::LivelinessLease(ms) => format!("{ms}ms"),
            QosStatement::Liveliness(k) => (*k).to_string(),
        }
    }
}

/// One override's statement about sizing: `(topic, role, statement)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QosStatementOverride {
    /// Resolved topic, e.g. `"/chatter"`.
    pub topic: String,
    /// [`qos_override_role`] code.
    pub role: u8,
    pub statement: QosStatement,
}

/// Read one parameter as a statement about SIZING.
///
/// Three outcomes, and keeping them apart is the point:
///
/// * `Ok(None)` — the parameter is not an override, or it is an override of a
///   policy that bounds occupancy rather than capacity ([`MODELLED_POLICIES`]'s
///   second column). Nothing to compare; the runtime table still bakes it.
/// * `Ok(Some(_))` — what this parameter says about a buffer's size.
/// * `Err(_)` — the same refusals [`lower`] makes, from the same parser and the
///   same value vocabulary. A caller that reached this before the bake must not
///   get a second opinion about whether the key is usable.
pub fn contract_statement(
    name: &str,
    value: &str,
) -> Result<Option<QosStatementOverride>, QoSOverrideError> {
    let Some((topic, role, policy_s)) = split_key(name)? else {
        return Ok(None);
    };
    let v = value.trim();
    let bad = |expected: &'static str| QoSOverrideError::BadValue {
        key: name.to_string(),
        policy: policy_s.to_string(),
        value: v.to_string(),
        expected,
    };
    let statement = match policy_s {
        "reliability" => {
            QosStatement::Reliability(parse_reliability(v).ok_or_else(|| bad(RELIABILITY_VALUES))?)
        }
        "durability" => {
            QosStatement::Durability(parse_durability(v).ok_or_else(|| bad(DURABILITY_VALUES))?)
        }
        "history" => QosStatement::History(parse_history(v).ok_or_else(|| bad(HISTORY_VALUES))?),
        "depth" => QosStatement::Depth(
            v.parse::<u32>()
                .map_err(|_| bad("a non-negative integer"))?,
        ),
        // phase-462 W5 -- the liveness policies the contract states too
        // (`deadline`, `lifespan`, `lease_duration`, `liveliness`). Same
        // refusals as `lower`: the same conversion, the same spellings.
        "deadline" => {
            QosStatement::Deadline(duration_ns_to_wire_ms(v).ok_or_else(|| bad(DURATION_NS))?)
        }
        "lifespan" => {
            QosStatement::Lifespan(duration_ns_to_wire_ms(v).ok_or_else(|| bad(DURATION_NS))?)
        }
        "liveliness_lease_duration" => QosStatement::LivelinessLease(
            duration_ns_to_wire_ms(v).ok_or_else(|| bad(DURATION_NS))?,
        ),
        "liveliness" => {
            QosStatement::Liveliness(parse_liveliness(v).ok_or_else(|| bad(LIVELINESS_VALUES))?)
        }
        _ => {
            return Err(QoSOverrideError::UnknownPolicy {
                key: name.to_string(),
                policy: policy_s.to_string(),
            });
        }
    };
    Ok(Some(QosStatementOverride {
        topic: topic.to_string(),
        role,
        statement,
    }))
}

/// Lower every QoS override in a parameter list, sorted for deterministic
/// emission. Non-override parameters are ignored; the FIRST unusable override
/// is an error, so a build never ships a half-applied QoS table.
pub fn lower_all<'a, I>(params: I) -> Result<Vec<LoweredOverride>, QoSOverrideError>
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let mut out = Vec::new();
    for (name, value) in params {
        if let Some(o) = lower(name, value)? {
            out.push(o);
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_modelled_policy_lowers() {
        let cases: &[(&str, &str, u8, u8, u32)] = &[
            (
                "qos_overrides./chatter.publisher.reliability",
                "best_effort",
                0,
                qos_override_policy::RELIABILITY,
                0,
            ),
            (
                "qos_overrides./chatter.subscription.durability",
                "transient_local",
                1,
                qos_override_policy::DURABILITY,
                1,
            ),
            (
                "qos_overrides./chatter.publisher.history",
                "keep_all",
                0,
                qos_override_policy::HISTORY,
                1,
            ),
            (
                "qos_overrides./chatter.publisher.depth",
                "7",
                0,
                qos_override_policy::DEPTH,
                7,
            ),
            (
                "qos_overrides./chatter.publisher.deadline",
                "100000000",
                0,
                qos_override_policy::DEADLINE,
                100,
            ),
            (
                "qos_overrides./chatter.publisher.lifespan",
                "250000000",
                0,
                qos_override_policy::LIFESPAN,
                250,
            ),
            (
                "qos_overrides./chatter.publisher.liveliness",
                "manual_by_topic",
                0,
                qos_override_policy::LIVELINESS,
                // Named, for the same reason the lowering above is: W5/B2
                // renumbered this enum to upstream's ordering and a literal
                // `2` here kept asserting the OTHER policy. A test that
                // hardcodes a discriminant cannot notice it moved.
                nros_rmw::QoSLivelinessPolicy::ManualByTopic as u32,
            ),
            (
                "qos_overrides./chatter.publisher.liveliness_lease_duration",
                "500000000",
                0,
                qos_override_policy::LIVELINESS_LEASE,
                500,
            ),
        ];
        for (name, value, role, policy, v) in cases {
            let got = lower(name, value)
                .unwrap_or_else(|e| panic!("{name} should lower: {e}"))
                .unwrap_or_else(|| panic!("{name} should be recognised as an override"));
            assert_eq!(
                got,
                LoweredOverride {
                    topic: "/chatter".to_string(),
                    role: *role,
                    policy: *policy,
                    value: *v,
                },
                "{name}"
            );
        }
    }

    /// Issue 1855 -- the duration policies read NANOSECONDS, the unit rclcpp
    /// and play_launch's resolver write, and bake MILLISECONDS.
    ///
    /// The two values the issue names: a 500 ms lease used to bake as
    /// 500,000,000 ms (about 5.8 days), and a 4.3 s deadline overflowed the
    /// `u32` and was refused with a value nobody wrote.
    #[test]
    fn duration_policies_read_nanoseconds_and_bake_milliseconds() {
        let bake = |policy: &str, value: &str| {
            lower(&format!("qos_overrides./t.subscription.{policy}"), value)
                .unwrap_or_else(|e| panic!("{policy}={value}: {e}"))
                .expect("an override")
                .value
        };
        assert_eq!(bake("liveliness_lease_duration", "500000000"), 500);
        assert_eq!(bake("deadline", "4300000000"), 4300);
        assert_eq!(bake("lifespan", "1000000"), 1, "one millisecond exactly");
        // A sub-millisecond remainder CEILS (`duration_to_qos_ms`): rounding
        // down would loosen the bound the author wrote.
        assert_eq!(bake("deadline", "1500000"), 2);
        // Zero stays "no deadline"; rclcpp's infinite is the sentinel.
        assert_eq!(bake("deadline", "0"), 0);
        assert_eq!(
            bake("deadline", "9223372036854775807"),
            nros_rmw::DURATION_INFINITE_MS
        );
    }

    /// Issue 1855 -- what the lowering cannot represent is REFUSED, never
    /// wrapped or rounded into a different bound.
    #[test]
    fn an_unrepresentable_duration_is_refused() {
        for (value, why) in [
            // The pre-1855 millisecond spelling: nonzero and under 1 ms.
            ("100", "a millisecond spelling"),
            ("999999", "just under the wire's resolution"),
            // At or past the u32 millisecond sentinel.
            (
                "4294967295000000",
                "the infinite sentinel's own millisecond count",
            ),
            ("-1", "negative"),
            ("1.5", "not an integer"),
        ] {
            let e = lower("qos_overrides./t.publisher.deadline", value)
                .expect_err(&format!("{value} ({why}) must be refused"));
            assert!(matches!(e, QoSOverrideError::BadValue { .. }), "{e:?}");
            assert!(e.to_string().contains("NANOSECONDS"), "{e}");
        }
    }

    /// A dotted topic keeps its dots: only the LAST two segments are role and
    /// policy.
    #[test]
    fn a_dotted_topic_survives() {
        let got = lower("qos_overrides./ns/a.b.publisher.depth", "3")
            .unwrap()
            .unwrap();
        assert_eq!(got.topic, "/ns/a.b");
    }

    /// phase-454 W3 -- every spelling round-trips through parse and back.
    ///
    /// The contract reader (`nros_cli_core::entity_inventory`) PARSES with
    /// these and RENDERS with `*_spelling`, so a round trip that lost a value
    /// would publish a policy nobody wrote. Both directions, and every variant
    /// including the `SystemDefault` no surface parses -- a renderer that
    /// panicked or emptied on it would be a hole a future producer falls into.
    #[test]
    fn every_policy_spelling_round_trips() {
        for (s, p) in [
            ("best_effort", QoSReliabilityPolicy::BestEffort),
            ("reliable", QoSReliabilityPolicy::Reliable),
        ] {
            assert_eq!(parse_reliability(s), Some(p), "{s}");
            assert_eq!(reliability_spelling(p), s);
        }
        for (s, p) in [
            ("volatile", QoSDurabilityPolicy::Volatile),
            ("transient_local", QoSDurabilityPolicy::TransientLocal),
        ] {
            assert_eq!(parse_durability(s), Some(p), "{s}");
            assert_eq!(durability_spelling(p), s);
        }
        for (s, p) in [
            ("keep_last", QoSHistoryPolicy::KeepLast),
            ("keep_all", QoSHistoryPolicy::KeepAll),
        ] {
            assert_eq!(parse_history(s), Some(p), "{s}");
            assert_eq!(history_spelling(p), s);
        }
        // `system_default` renders and does NOT parse: an author who writes it
        // gets a refusal naming the two spellings, rather than an endpoint that
        // silently means "the backend picks". See the module note above.
        assert_eq!(
            reliability_spelling(QoSReliabilityPolicy::SystemDefault),
            "system_default"
        );
        assert_eq!(parse_reliability("system_default"), None);
        assert_eq!(parse_durability("system_default"), None);
        assert_eq!(parse_history("system_default"), None);
    }

    /// phase-454 W3 -- a spelling neither surface models is `None`, and
    /// `lower` turns that `None` into an error rather than a value.
    ///
    /// The binding between the two halves: the contract road refuses on the
    /// `None`, and this road refuses on the same `None`. One vocabulary means a
    /// spelling cannot be legal on one surface and silently dropped on the
    /// other -- which is what a second `match v { "best_effort" => .. }` here
    /// would eventually become.
    #[test]
    fn a_spelling_neither_surface_models_is_refused_on_both() {
        for bad in ["best-effort", "BestEffort", "BEST_EFFORT", "", "reliabel"] {
            assert_eq!(parse_reliability(bad), None, "{bad}");
            let e = lower("qos_overrides./t.publisher.reliability", bad).unwrap_err();
            assert!(
                matches!(e, QoSOverrideError::BadValue { .. }),
                "{bad}: {e:?}"
            );
            assert!(e.to_string().contains("best_effort"), "{e}");
        }
    }

    /// The whole point of issue 0303: every rejection is an ERROR naming the
    /// key, not a silent skip.
    #[test]
    fn unusable_overrides_are_errors_not_silence() {
        // `pub` instead of `publisher` — the typo that used to vanish.
        let e = lower("qos_overrides./t.pub.reliability", "reliable").unwrap_err();
        assert!(matches!(e, QoSOverrideError::UnknownRole { .. }), "{e:?}");
        assert!(e.to_string().contains("publisher"), "{e}");

        // A policy this build does not model.
        let e = lower("qos_overrides./t.publisher.bandwidth", "10").unwrap_err();
        assert!(matches!(e, QoSOverrideError::UnknownPolicy { .. }), "{e:?}");
        assert!(e.to_string().contains("deadline"), "{e}");

        // Right policy, wrong value.
        let e = lower("qos_overrides./t.publisher.reliability", "relaible").unwrap_err();
        assert!(matches!(e, QoSOverrideError::BadValue { .. }), "{e:?}");
        let e = lower("qos_overrides./t.publisher.depth", "lots").unwrap_err();
        assert!(matches!(e, QoSOverrideError::BadValue { .. }), "{e:?}");

        // Prefix present, shape wrong.
        for key in [
            "qos_overrides.",
            "qos_overrides./t",
            "qos_overrides./t.publisher",
            "qos_overrides./t..depth",
        ] {
            let e = lower(key, "1").unwrap_err();
            assert!(
                matches!(
                    e,
                    QoSOverrideError::Malformed { .. } | QoSOverrideError::UnknownRole { .. }
                ),
                "{key}: {e:?}"
            );
        }
    }

    /// A parameter without the prefix is not an override — `Ok(None)`, so a
    /// caller can walk a mixed list.
    #[test]
    fn ordinary_parameters_are_not_overrides() {
        assert_eq!(lower("use_sim_time", "true").unwrap(), None);
        assert!(!is_qos_override("use_sim_time"));
        assert!(is_qos_override("qos_overrides./t.publisher.depth"));
    }

    #[test]
    fn lower_all_sorts_and_skips_ordinary_params() {
        let got = lower_all([
            ("qos_overrides./z.publisher.depth", "1"),
            ("use_sim_time", "true"),
            ("qos_overrides./a.publisher.depth", "2"),
        ])
        .unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].topic, "/a");
        assert_eq!(got[1].topic, "/z");
    }

    /// phase-454 W7 -- `lower` and `contract_statement` accept exactly the same
    /// policy names, and [`MODELLED_POLICIES`] lists exactly those.
    ///
    /// Three readers of one vocabulary: the bake, the sizing comparison, and
    /// the `POLICY_NAMES` diagnostic. A policy added to `lower` and forgotten
    /// in the table is one nobody answered the capacity question for -- it
    /// would silently become "not a capacity", which is the safe-looking
    /// direction and the wrong one. This is the test that makes it a decision.
    #[test]
    fn every_lowerable_policy_is_classified_for_sizing() {
        // A value that parses for whichever policy it is handed to. `1` is a
        // legal depth; the durations are nanoseconds since issue 1855, so they
        // get one millisecond; the enum policies get their own spellings.
        let value_for = |policy: &str| match policy {
            "reliability" => "reliable",
            "durability" => "volatile",
            "history" => "keep_last",
            "liveliness" => "automatic",
            "deadline" | "lifespan" | "liveliness_lease_duration" => "1000000",
            _ => "1",
        };
        for (policy, is_capacity) in MODELLED_POLICIES {
            let key = format!("qos_overrides./t.publisher.{policy}");
            let v = value_for(policy);
            lower(&key, v)
                .unwrap_or_else(|e| panic!("{policy} must lower: {e}"))
                .unwrap_or_else(|| panic!("{policy} must be recognised as an override"));
            // phase-462 W5 -- every modelled policy is a contract statement
            // now; the capacity column decides only how a disagreement reads.
            let sized = contract_statement(&key, v)
                .unwrap_or_else(|e| panic!("{policy} must classify: {e}"))
                .unwrap_or_else(|| panic!("{policy} must be a contract statement"));
            assert_eq!(sized.statement.policy(), policy);
            assert_eq!(
                sized.statement.is_capacity(),
                is_capacity,
                "`{policy}` is classified {} by MODELLED_POLICIES and the other way by \
                 QosStatement::is_capacity",
                if is_capacity { "capacity" } else { "occupancy" }
            );
            assert!(
                POLICY_NAMES.contains(policy),
                "`{policy}` is modelled but the diagnostic does not list it"
            );
        }
        // …and nothing outside the table lowers, so the two lists cannot drift
        // by an ADDITION to `lower` either.
        for policy in ["bandwidth", "reliability_", "", "Depth"] {
            let key = format!("qos_overrides./t.publisher.{policy}");
            assert!(lower(&key, "1").is_err(), "`{policy}` must not lower");
        }
    }

    /// phase-454 W7 -- the two readers agree about the VALUE, not just the
    /// name.
    ///
    /// `lower` produces the wire's `0`/`1` and `contract_statement` produces the
    /// variant; they are two encodings of one authored value, so this asserts
    /// the pairing rather than either half. A renumbered wire code or a
    /// mis-mapped spelling shows up here as a mismatched pair.
    #[test]
    fn the_bake_and_the_contract_statement_read_one_value() {
        let cases: &[(&str, &str, QosStatement, u32)] = &[
            (
                "reliability",
                "best_effort",
                QosStatement::Reliability(QoSReliabilityPolicy::BestEffort),
                0,
            ),
            (
                "reliability",
                "reliable",
                QosStatement::Reliability(QoSReliabilityPolicy::Reliable),
                1,
            ),
            (
                "durability",
                "volatile",
                QosStatement::Durability(QoSDurabilityPolicy::Volatile),
                0,
            ),
            (
                "durability",
                "transient_local",
                QosStatement::Durability(QoSDurabilityPolicy::TransientLocal),
                1,
            ),
            (
                "history",
                "keep_last",
                QosStatement::History(QoSHistoryPolicy::KeepLast),
                0,
            ),
            (
                "history",
                "keep_all",
                QosStatement::History(QoSHistoryPolicy::KeepAll),
                1,
            ),
            ("depth", "64", QosStatement::Depth(64), 64),
        ];
        for (policy, written, statement, wire) in cases {
            let key = format!("qos_overrides./chatter.subscription.{policy}");
            let lowered = lower(&key, written).unwrap().unwrap();
            let sized = contract_statement(&key, written).unwrap().unwrap();
            assert_eq!(lowered.value, *wire, "{policy}={written} wire code");
            assert_eq!(sized.statement, *statement, "{policy}={written} statement");
            assert_eq!(sized.topic, lowered.topic);
            assert_eq!(sized.role, lowered.role);
            assert_eq!(
                sized.statement.value(),
                *written,
                "the statement must render back to what the author wrote"
            );
        }
    }

    /// phase-462 W5 -- the liveness policies: the statement carries the wire's
    /// milliseconds by the SAME conversion `lower` bakes, a contract
    /// millisecond value converts to the same number, and the contract key
    /// for the lease is `lease_duration`.
    #[test]
    fn a_liveness_statement_is_the_baked_value() {
        for (policy, written, ms) in [
            ("deadline", "100000000", 100u32),
            ("lifespan", "2500000000", 2_500),
            ("liveliness_lease_duration", "500000000", 500),
        ] {
            let key = format!("qos_overrides./t.subscription.{policy}");
            let lowered = lower(&key, written).unwrap().unwrap();
            let st = contract_statement(&key, written)
                .unwrap()
                .unwrap()
                .statement;
            assert_eq!(lowered.value, ms, "{policy}");
            assert_eq!(st.value(), format!("{ms}ms"), "{policy}");
            assert!(!st.is_capacity());
            assert_eq!(contract_ms_to_wire_ms(f64::from(ms)), Some(ms), "{policy}");
        }
        let lease = contract_statement(
            "qos_overrides./t.subscription.liveliness_lease_duration",
            "1000000",
        )
        .unwrap()
        .unwrap()
        .statement;
        assert_eq!(lease.contract_key(), "lease_duration");
        let kind = contract_statement("qos_overrides./t.publisher.liveliness", "manual_by_topic")
            .unwrap()
            .unwrap()
            .statement;
        assert_eq!(kind, QosStatement::Liveliness("manual_by_topic"));
        assert!(contract_statement("qos_overrides./t.publisher.liveliness", "auto").is_err());
        assert!(contract_statement("qos_overrides./t.publisher.deadline", "100").is_err());
        assert_eq!(contract_ms_to_wire_ms(-1.0), None);
    }

    /// phase-454 W7 -- the shared key parser refuses the same keys on both
    /// roads. A sizing comparison that accepted `pub` where the bake refuses it
    /// would compare an endpoint nothing runs.
    #[test]
    fn both_readers_refuse_the_same_keys() {
        for key in [
            "qos_overrides.",
            "qos_overrides./t",
            "qos_overrides./t.publisher",
            "qos_overrides./t..depth",
            "qos_overrides./t.pub.depth",
        ] {
            assert!(lower(key, "1").is_err(), "{key} must not lower");
            assert!(
                contract_statement(key, "1").is_err(),
                "{key} must not classify"
            );
        }
        assert_eq!(contract_statement("use_sim_time", "true").unwrap(), None);
    }

    /// One bad override fails the whole list — a half-applied QoS table is
    /// worse than a failed build.
    #[test]
    fn lower_all_fails_on_the_first_bad_override() {
        let e = lower_all([
            ("qos_overrides./a.publisher.depth", "1"),
            ("qos_overrides./b.publisher.nonsense", "1"),
        ])
        .unwrap_err();
        assert!(matches!(e, QoSOverrideError::UnknownPolicy { .. }), "{e:?}");
    }
}
