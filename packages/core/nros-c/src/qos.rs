//! QoS (Quality of Service) settings for the C API.

use core::ffi::c_int;

/// QoS reliability policy
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_qos_reliability_t {
    /// Best effort delivery - no guarantees
    NROS_QOS_RELIABILITY_BEST_EFFORT = 0,
    /// Reliable delivery - retransmit if needed
    NROS_QOS_RELIABILITY_RELIABLE = 1,
}

/// QoS durability policy
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_qos_durability_t {
    /// Volatile - no persistence
    NROS_QOS_DURABILITY_VOLATILE = 0,
    /// Transient local - persist for late joiners
    NROS_QOS_DURABILITY_TRANSIENT_LOCAL = 1,
}

/// QoS history policy
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_qos_history_t {
    /// Keep last N samples
    NROS_QOS_HISTORY_KEEP_LAST = 0,
    /// Keep all samples
    NROS_QOS_HISTORY_KEEP_ALL = 1,
}

/// QoS liveliness policy. Phase 109 — matches DDS `LIVELINESS`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_qos_liveliness_t {
    /// No liveliness assertion or tracking.
    NROS_QOS_LIVELINESS_NONE = 0,
    /// Backend's keepalive task asserts liveliness automatically.
    NROS_QOS_LIVELINESS_AUTOMATIC = 1,
    /// Application calls `assert_liveliness()` per topic explicitly.
    NROS_QOS_LIVELINESS_MANUAL_BY_TOPIC = 2,
    /// Application calls `assert_liveliness()` at the node level.
    NROS_QOS_LIVELINESS_MANUAL_BY_NODE = 3,
}

/// Full DDS-shaped QoS profile (Phase 109).
///
/// Matches the field set of upstream `rmw_qos_profile_t`. Backends
/// advertise per-policy support; entities created with a profile the
/// active backend can't honour return `NROS_RMW_RET_INCOMPATIBLE_QOS`
/// synchronously at create time — no silent downgrade.
///
/// Zero-valued time-window fields ("off") preserve a cheap default
/// for apps that don't request the policy.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct nros_qos_t {
    /// Reliability policy
    pub reliability: nros_qos_reliability_t,
    /// Durability policy
    pub durability: nros_qos_durability_t,
    /// History policy
    pub history: nros_qos_history_t,
    /// Liveliness policy
    pub liveliness_kind: nros_qos_liveliness_t,
    /// History depth (for KEEP_LAST)
    pub depth: c_int,
    /// Subscriber max-inter-arrival / publisher offered-rate, ms.
    /// `0` = infinite (no deadline check).
    pub deadline_ms: u32,
    /// Sample expiry, ms. `0` = infinite.
    pub lifespan_ms: u32,
    /// Liveliness lease, ms. `0` = infinite.
    pub liveliness_lease_ms: u32,
    /// If non-zero, topic-name encoding skips the `/rt/` ROS prefix.
    pub avoid_ros_namespace_conventions: u8,
    /// Phase 282 (#145) — publisher-side "express" hint: if non-zero, this
    /// publisher's samples bypass transport tx batching (sent immediately
    /// even when the batching knob is on). A transport hint, not a DDS
    /// policy; ignored on subscriptions and by backends without batching.
    pub tx_express: u8,
}

impl Default for nros_qos_t {
    fn default() -> Self {
        Self {
            reliability: nros_qos_reliability_t::NROS_QOS_RELIABILITY_RELIABLE,
            durability: nros_qos_durability_t::NROS_QOS_DURABILITY_VOLATILE,
            history: nros_qos_history_t::NROS_QOS_HISTORY_KEEP_LAST,
            liveliness_kind: nros_qos_liveliness_t::NROS_QOS_LIVELINESS_AUTOMATIC,
            depth: 10,
            deadline_ms: 0,
            lifespan_ms: 0,
            liveliness_lease_ms: 0,
            avoid_ros_namespace_conventions: 0,
            tx_express: 0,
        }
    }
}

/// Default QoS profile (matches `rmw_qos_profile_default`).
#[unsafe(no_mangle)]
pub static NROS_QOS_DEFAULT: nros_qos_t = nros_qos_t {
    reliability: nros_qos_reliability_t::NROS_QOS_RELIABILITY_RELIABLE,
    durability: nros_qos_durability_t::NROS_QOS_DURABILITY_VOLATILE,
    history: nros_qos_history_t::NROS_QOS_HISTORY_KEEP_LAST,
    liveliness_kind: nros_qos_liveliness_t::NROS_QOS_LIVELINESS_AUTOMATIC,
    depth: 10,
    deadline_ms: 0,
    lifespan_ms: 0,
    liveliness_lease_ms: 0,
    avoid_ros_namespace_conventions: 0,
    tx_express: 0,
};

/// Sensor data QoS profile (best effort, small depth).
#[unsafe(no_mangle)]
pub static NROS_QOS_SENSOR_DATA: nros_qos_t = nros_qos_t {
    reliability: nros_qos_reliability_t::NROS_QOS_RELIABILITY_BEST_EFFORT,
    durability: nros_qos_durability_t::NROS_QOS_DURABILITY_VOLATILE,
    history: nros_qos_history_t::NROS_QOS_HISTORY_KEEP_LAST,
    liveliness_kind: nros_qos_liveliness_t::NROS_QOS_LIVELINESS_AUTOMATIC,
    depth: 5,
    deadline_ms: 0,
    lifespan_ms: 0,
    liveliness_lease_ms: 0,
    avoid_ros_namespace_conventions: 0,
    tx_express: 0,
};

/// Services QoS profile (reliable).
#[unsafe(no_mangle)]
pub static NROS_QOS_SERVICES: nros_qos_t = nros_qos_t {
    reliability: nros_qos_reliability_t::NROS_QOS_RELIABILITY_RELIABLE,
    durability: nros_qos_durability_t::NROS_QOS_DURABILITY_VOLATILE,
    history: nros_qos_history_t::NROS_QOS_HISTORY_KEEP_LAST,
    liveliness_kind: nros_qos_liveliness_t::NROS_QOS_LIVELINESS_AUTOMATIC,
    depth: 10,
    deadline_ms: 0,
    lifespan_ms: 0,
    liveliness_lease_ms: 0,
    avoid_ros_namespace_conventions: 0,
    tx_express: 0,
};

impl nros_qos_t {
    /// Convert to nros QosSettings
    pub(crate) fn to_qos_settings(self) -> nros_node::QosSettings {
        use nros_node::{
            QosDurabilityPolicy, QosHistoryPolicy, QosLivelinessPolicy, QosReliabilityPolicy,
        };

        let reliability = match self.reliability {
            nros_qos_reliability_t::NROS_QOS_RELIABILITY_BEST_EFFORT => {
                QosReliabilityPolicy::BestEffort
            }
            nros_qos_reliability_t::NROS_QOS_RELIABILITY_RELIABLE => QosReliabilityPolicy::Reliable,
        };

        let durability = match self.durability {
            nros_qos_durability_t::NROS_QOS_DURABILITY_VOLATILE => QosDurabilityPolicy::Volatile,
            nros_qos_durability_t::NROS_QOS_DURABILITY_TRANSIENT_LOCAL => {
                QosDurabilityPolicy::TransientLocal
            }
        };

        let history = match self.history {
            nros_qos_history_t::NROS_QOS_HISTORY_KEEP_LAST => QosHistoryPolicy::KeepLast,
            nros_qos_history_t::NROS_QOS_HISTORY_KEEP_ALL => QosHistoryPolicy::KeepAll,
        };

        let liveliness_kind = match self.liveliness_kind {
            nros_qos_liveliness_t::NROS_QOS_LIVELINESS_NONE => QosLivelinessPolicy::None,
            nros_qos_liveliness_t::NROS_QOS_LIVELINESS_AUTOMATIC => QosLivelinessPolicy::Automatic,
            nros_qos_liveliness_t::NROS_QOS_LIVELINESS_MANUAL_BY_TOPIC => {
                QosLivelinessPolicy::ManualByTopic
            }
            nros_qos_liveliness_t::NROS_QOS_LIVELINESS_MANUAL_BY_NODE => {
                QosLivelinessPolicy::ManualByNode
            }
        };

        nros_node::QosSettings {
            reliability,
            durability,
            history,
            liveliness_kind,
            depth: self.depth as u32,
            deadline_ms: self.deadline_ms,
            lifespan_ms: self.lifespan_ms,
            liveliness_lease_ms: self.liveliness_lease_ms,
            avoid_ros_namespace_conventions: self.avoid_ros_namespace_conventions != 0,
            tx_express: self.tx_express != 0,
        }
    }
}

/// Phase 211.H (issue #52) — one per-topic QoS override, the C-ABI mirror of
/// Rust's `nros_rmw::QosOverride`. The deploy plan lowers a
/// `qos_overrides.<topic>.<role>.<policy>` launch param into a `&'static`
/// array of these, which the entry installs on the node via
/// [`nros_node_set_qos_overrides`](crate::node::nros_node_set_qos_overrides);
/// the node folds the matching entries into each entity's QoS at
/// `create_publisher` / `create_subscription` time (setup-time, before the
/// backend-compat check — no silent downgrade).
///
/// Plain scalar fields only (no `#[repr(C)]` enums) so the C++/cbindgen header
/// is trivially stable and there is no short-enum ABI mirror to keep in sync.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct nros_qos_override_t {
    /// Resolved (remapped) topic the override targets, NUL-terminated UTF-8
    /// (e.g. `"/chatter"`). Matched exactly against the entity's topic.
    pub topic: *const core::ffi::c_char,
    /// `0` = publisher, `1` = subscription. Other values never match.
    pub role: u8,
    /// `0` = reliability, `1` = durability, `2` = history, `3` = depth.
    pub policy: u8,
    /// Policy-specific value: reliability `0`=best_effort/`1`=reliable;
    /// durability `0`=volatile/`1`=transient_local; history
    /// `0`=keep_last/`1`=keep_all; depth = the KeepLast depth.
    pub value: u32,
}

/// Role tag for [`apply_qos_overrides`]: `0` = publisher, `1` = subscription
/// (matches [`nros_qos_override_t::role`]).
pub(crate) const QOS_OVERRIDE_ROLE_PUBLISHER: u8 = 0;
pub(crate) const QOS_OVERRIDE_ROLE_SUBSCRIPTION: u8 = 1;

/// Fold any overrides matching `(topic, role)` into `qos`, returning the
/// overridden profile. Mirrors `nros_rmw::QosSettings::apply_overrides`: a
/// single linear scan, last-write-wins on a duplicate `(topic, role, policy)`,
/// no alloc. `overrides` may be null (`len == 0` ⇒ no-op).
///
/// # Safety
/// `overrides` must be null or point to `len` valid `nros_qos_override_t`, each
/// with a `topic` that is null or a valid NUL-terminated UTF-8 C string for the
/// duration of the call.
pub(crate) unsafe fn apply_qos_overrides(
    mut qos: nros_node::QosSettings,
    overrides: *const nros_qos_override_t,
    len: usize,
    topic: &str,
    role: u8,
) -> nros_node::QosSettings {
    use nros_node::{QosDurabilityPolicy, QosHistoryPolicy, QosReliabilityPolicy};

    if overrides.is_null() || len == 0 {
        return qos;
    }
    let table = unsafe { core::slice::from_raw_parts(overrides, len) };
    for ovr in table {
        if ovr.role != role || ovr.topic.is_null() {
            continue;
        }
        let ovr_topic = match unsafe { core::ffi::CStr::from_ptr(ovr.topic) }.to_str() {
            Ok(s) => s,
            Err(_) => continue,
        };
        if ovr_topic != topic {
            continue;
        }
        match ovr.policy {
            0 => {
                qos.reliability = if ovr.value == 0 {
                    QosReliabilityPolicy::BestEffort
                } else {
                    QosReliabilityPolicy::Reliable
                }
            }
            1 => {
                qos.durability = if ovr.value == 0 {
                    QosDurabilityPolicy::Volatile
                } else {
                    QosDurabilityPolicy::TransientLocal
                }
            }
            2 => {
                qos.history = if ovr.value == 0 {
                    QosHistoryPolicy::KeepLast
                } else {
                    QosHistoryPolicy::KeepAll
                }
            }
            3 => qos.depth = ovr.value,
            _ => {}
        }
    }
    qos
}

#[cfg(test)]
mod tests {
    use super::*;
    use nros_node::{QosDurabilityPolicy, QosReliabilityPolicy};

    #[test]
    fn apply_qos_overrides_matches_topic_and_role() {
        // best_effort reliability on /chatter for the publisher role.
        let ovr = [nros_qos_override_t {
            topic: c"/chatter".as_ptr(),
            role: QOS_OVERRIDE_ROLE_PUBLISHER,
            policy: 0, // reliability
            value: 0,  // best_effort
        }];
        let base = nros_node::QosSettings::default(); // Reliable

        // Matching (topic, role) → overridden.
        let got = unsafe {
            apply_qos_overrides(
                base,
                ovr.as_ptr(),
                ovr.len(),
                "/chatter",
                QOS_OVERRIDE_ROLE_PUBLISHER,
            )
        };
        assert_eq!(got.reliability, QosReliabilityPolicy::BestEffort);

        // Wrong role → untouched.
        let got = unsafe {
            apply_qos_overrides(
                base,
                ovr.as_ptr(),
                ovr.len(),
                "/chatter",
                QOS_OVERRIDE_ROLE_SUBSCRIPTION,
            )
        };
        assert_eq!(got.reliability, QosReliabilityPolicy::Reliable);

        // Wrong topic → untouched.
        let got = unsafe {
            apply_qos_overrides(
                base,
                ovr.as_ptr(),
                ovr.len(),
                "/other",
                QOS_OVERRIDE_ROLE_PUBLISHER,
            )
        };
        assert_eq!(got.reliability, QosReliabilityPolicy::Reliable);

        // Null / empty table → no-op.
        let got = unsafe {
            apply_qos_overrides(
                base,
                core::ptr::null(),
                0,
                "/chatter",
                QOS_OVERRIDE_ROLE_PUBLISHER,
            )
        };
        assert_eq!(got.reliability, QosReliabilityPolicy::Reliable);
    }

    #[test]
    fn apply_qos_overrides_durability_and_depth() {
        let ovr = [
            nros_qos_override_t {
                topic: c"/t".as_ptr(),
                role: QOS_OVERRIDE_ROLE_SUBSCRIPTION,
                policy: 1, // durability
                value: 1,  // transient_local
            },
            nros_qos_override_t {
                topic: c"/t".as_ptr(),
                role: QOS_OVERRIDE_ROLE_SUBSCRIPTION,
                policy: 3, // depth
                value: 42,
            },
        ];
        let got = unsafe {
            apply_qos_overrides(
                nros_node::QosSettings::default(),
                ovr.as_ptr(),
                ovr.len(),
                "/t",
                QOS_OVERRIDE_ROLE_SUBSCRIPTION,
            )
        };
        assert_eq!(got.durability, QosDurabilityPolicy::TransientLocal);
        assert_eq!(got.depth, 42);
    }
}
