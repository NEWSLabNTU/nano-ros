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
/// [`nros_rmw::QosOverrideCode`] but owning its topic (the producers turn it
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
pub enum QosOverrideError {
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

impl fmt::Display for QosOverrideError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QosOverrideError::Malformed { key } => write!(
                f,
                "parameter `{key}` looks like a QoS override but is not \
                 `qos_overrides.<topic>.<role>.<policy>`"
            ),
            QosOverrideError::UnknownRole { key, role } => write!(
                f,
                "parameter `{key}`: unknown QoS override role `{role}` \
                 (expected `publisher` or `subscription`)"
            ),
            QosOverrideError::UnknownPolicy { key, policy } => write!(
                f,
                "parameter `{key}`: unknown QoS override policy `{policy}` (expected one of: \
                 {POLICY_NAMES})"
            ),
            QosOverrideError::BadValue {
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

impl std::error::Error for QosOverrideError {}

/// The policy spellings this build models, for diagnostics.
const POLICY_NAMES: &str = "reliability, durability, history, depth, deadline, lifespan, \
                            liveliness, liveliness_lease_duration";

/// Is this parameter name a QoS override?
pub fn is_qos_override(name: &str) -> bool {
    name.starts_with(QOS_OVERRIDE_PREFIX)
}

/// Lower one `qos_overrides.<topic>.<role>.<policy>` parameter.
///
/// Returns `Ok(None)` when `name` is not a QoS override at all — the caller is
/// walking a mixed parameter list. A name that DOES carry the prefix but is
/// unusable is an `Err`, never a skip.
pub fn lower(name: &str, value: &str) -> Result<Option<LoweredOverride>, QosOverrideError> {
    let Some(rest) = name.strip_prefix(QOS_OVERRIDE_PREFIX) else {
        return Ok(None);
    };
    let key = name.to_string();

    // rsplitn(3, '.') → [policy, role, topic]: the topic may itself contain
    // dots, the trailing two segments may not.
    let mut parts = rest.rsplitn(3, '.');
    let (Some(policy_s), Some(role_s), Some(topic)) = (parts.next(), parts.next(), parts.next())
    else {
        return Err(QosOverrideError::Malformed { key });
    };
    if topic.is_empty() || role_s.is_empty() || policy_s.is_empty() {
        return Err(QosOverrideError::Malformed { key });
    }

    let role = match role_s {
        "publisher" => qos_override_role::PUBLISHER,
        "subscription" => qos_override_role::SUBSCRIPTION,
        _ => {
            return Err(QosOverrideError::UnknownRole {
                key,
                role: role_s.to_string(),
            });
        }
    };

    let v = value.trim();
    let bad = |expected: &'static str| QosOverrideError::BadValue {
        key: name.to_string(),
        policy: policy_s.to_string(),
        value: v.to_string(),
        expected,
    };
    let ms = |expected| v.parse::<u32>().map_err(|_| bad(expected));

    let (policy, value) = match policy_s {
        "reliability" => (
            qos_override_policy::RELIABILITY,
            match v {
                "best_effort" => 0,
                "reliable" => 1,
                _ => return Err(bad("`best_effort` or `reliable`")),
            },
        ),
        "durability" => (
            qos_override_policy::DURABILITY,
            match v {
                "volatile" => 0,
                "transient_local" => 1,
                _ => return Err(bad("`volatile` or `transient_local`")),
            },
        ),
        "history" => (
            qos_override_policy::HISTORY,
            match v {
                "keep_last" => 0,
                "keep_all" => 1,
                _ => return Err(bad("`keep_last` or `keep_all`")),
            },
        ),
        "depth" => (qos_override_policy::DEPTH, ms("a non-negative integer")?),
        "deadline" => (
            qos_override_policy::DEADLINE,
            ms("a duration in milliseconds")?,
        ),
        "lifespan" => (
            qos_override_policy::LIFESPAN,
            ms("a duration in milliseconds")?,
        ),
        "liveliness" => (
            qos_override_policy::LIVELINESS,
            match v {
                // The discriminants of `nros_rmw::QosLivelinessPolicy`, NAMED
                // rather than written out. Phase 376 W5/B2 renumbered that enum
                // to upstream's ordering (MANUAL_BY_NODE 3 -> 2,
                // MANUAL_BY_TOPIC 2 -> 3) and these literals kept compiling
                // while meaning the other policy. The decoder in
                // `nros_rmw::traits` is the other half of this wire; naming the
                // variant is what keeps the two ends from drifting apart, and
                // the comment claiming they were discriminants was the only
                // thing binding them before.
                "none" => nros_rmw::QosLivelinessPolicy::None as u32,
                "automatic" => nros_rmw::QosLivelinessPolicy::Automatic as u32,
                "manual_by_topic" => nros_rmw::QosLivelinessPolicy::ManualByTopic as u32,
                "manual_by_node" => nros_rmw::QosLivelinessPolicy::ManualByNode as u32,
                _ => {
                    return Err(bad(
                        "`none`, `automatic`, `manual_by_topic` or `manual_by_node`",
                    ));
                }
            },
        ),
        "liveliness_lease_duration" => (
            qos_override_policy::LIVELINESS_LEASE,
            ms("a duration in milliseconds")?,
        ),
        _ => {
            return Err(QosOverrideError::UnknownPolicy {
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

/// Lower every QoS override in a parameter list, sorted for deterministic
/// emission. Non-override parameters are ignored; the FIRST unusable override
/// is an error, so a build never ships a half-applied QoS table.
pub fn lower_all<'a, I>(params: I) -> Result<Vec<LoweredOverride>, QosOverrideError>
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
                "100",
                0,
                qos_override_policy::DEADLINE,
                100,
            ),
            (
                "qos_overrides./chatter.publisher.lifespan",
                "250",
                0,
                qos_override_policy::LIFESPAN,
                250,
            ),
            (
                "qos_overrides./chatter.publisher.liveliness",
                "manual_by_topic",
                0,
                qos_override_policy::LIVELINESS,
                2,
            ),
            (
                "qos_overrides./chatter.publisher.liveliness_lease_duration",
                "500",
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

    /// A dotted topic keeps its dots: only the LAST two segments are role and
    /// policy.
    #[test]
    fn a_dotted_topic_survives() {
        let got = lower("qos_overrides./ns/a.b.publisher.depth", "3")
            .unwrap()
            .unwrap();
        assert_eq!(got.topic, "/ns/a.b");
    }

    /// The whole point of issue 0303: every rejection is an ERROR naming the
    /// key, not a silent skip.
    #[test]
    fn unusable_overrides_are_errors_not_silence() {
        // `pub` instead of `publisher` — the typo that used to vanish.
        let e = lower("qos_overrides./t.pub.reliability", "reliable").unwrap_err();
        assert!(matches!(e, QosOverrideError::UnknownRole { .. }), "{e:?}");
        assert!(e.to_string().contains("publisher"), "{e}");

        // A policy this build does not model.
        let e = lower("qos_overrides./t.publisher.bandwidth", "10").unwrap_err();
        assert!(matches!(e, QosOverrideError::UnknownPolicy { .. }), "{e:?}");
        assert!(e.to_string().contains("deadline"), "{e}");

        // Right policy, wrong value.
        let e = lower("qos_overrides./t.publisher.reliability", "relaible").unwrap_err();
        assert!(matches!(e, QosOverrideError::BadValue { .. }), "{e:?}");
        let e = lower("qos_overrides./t.publisher.depth", "lots").unwrap_err();
        assert!(matches!(e, QosOverrideError::BadValue { .. }), "{e:?}");

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
                    QosOverrideError::Malformed { .. } | QosOverrideError::UnknownRole { .. }
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

    /// One bad override fails the whole list — a half-applied QoS table is
    /// worse than a failed build.
    #[test]
    fn lower_all_fails_on_the_first_bad_override() {
        let e = lower_all([
            ("qos_overrides./a.publisher.depth", "1"),
            ("qos_overrides./b.publisher.nonsense", "1"),
        ])
        .unwrap_err();
        assert!(matches!(e, QosOverrideError::UnknownPolicy { .. }), "{e:?}");
    }
}
