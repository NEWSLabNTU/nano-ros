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
        }
    }
}
