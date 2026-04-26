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

/// QoS settings structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct nros_qos_t {
    /// Reliability policy
    pub reliability: nros_qos_reliability_t,
    /// Durability policy
    pub durability: nros_qos_durability_t,
    /// History policy
    pub history: nros_qos_history_t,
    /// History depth (for KEEP_LAST)
    pub depth: c_int,
}

impl Default for nros_qos_t {
    fn default() -> Self {
        Self {
            reliability: nros_qos_reliability_t::NROS_QOS_RELIABILITY_RELIABLE,
            durability: nros_qos_durability_t::NROS_QOS_DURABILITY_VOLATILE,
            history: nros_qos_history_t::NROS_QOS_HISTORY_KEEP_LAST,
            depth: 10,
        }
    }
}

/// Default QoS profile
#[unsafe(no_mangle)]
pub static NROS_QOS_DEFAULT: nros_qos_t = nros_qos_t {
    reliability: nros_qos_reliability_t::NROS_QOS_RELIABILITY_RELIABLE,
    durability: nros_qos_durability_t::NROS_QOS_DURABILITY_VOLATILE,
    history: nros_qos_history_t::NROS_QOS_HISTORY_KEEP_LAST,
    depth: 10,
};

/// Sensor data QoS profile (best effort, small depth)
#[unsafe(no_mangle)]
pub static NROS_QOS_SENSOR_DATA: nros_qos_t = nros_qos_t {
    reliability: nros_qos_reliability_t::NROS_QOS_RELIABILITY_BEST_EFFORT,
    durability: nros_qos_durability_t::NROS_QOS_DURABILITY_VOLATILE,
    history: nros_qos_history_t::NROS_QOS_HISTORY_KEEP_LAST,
    depth: 5,
};

/// Services QoS profile (reliable)
#[unsafe(no_mangle)]
pub static NROS_QOS_SERVICES: nros_qos_t = nros_qos_t {
    reliability: nros_qos_reliability_t::NROS_QOS_RELIABILITY_RELIABLE,
    durability: nros_qos_durability_t::NROS_QOS_DURABILITY_VOLATILE,
    history: nros_qos_history_t::NROS_QOS_HISTORY_KEEP_LAST,
    depth: 10,
};

impl nros_qos_t {
    /// Convert to nros QosSettings
    pub(crate) fn to_qos_settings(self) -> nros_node::QosSettings {
        use nros_node::{QosDurabilityPolicy, QosHistoryPolicy, QosReliabilityPolicy};

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

        nros_node::QosSettings {
            reliability,
            durability,
            history,
            depth: self.depth as u32,
        }
    }
}
