//! QoS (Quality of Service) settings for the C API.

use core::ffi::c_int;

/// QoS reliability policy
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nano_ros_qos_reliability_t {
    /// Best effort delivery - no guarantees
    NANO_ROS_QOS_RELIABILITY_BEST_EFFORT = 0,
    /// Reliable delivery - retransmit if needed
    NANO_ROS_QOS_RELIABILITY_RELIABLE = 1,
}

/// QoS durability policy
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nano_ros_qos_durability_t {
    /// Volatile - no persistence
    NANO_ROS_QOS_DURABILITY_VOLATILE = 0,
    /// Transient local - persist for late joiners
    NANO_ROS_QOS_DURABILITY_TRANSIENT_LOCAL = 1,
}

/// QoS history policy
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nano_ros_qos_history_t {
    /// Keep last N samples
    NANO_ROS_QOS_HISTORY_KEEP_LAST = 0,
    /// Keep all samples
    NANO_ROS_QOS_HISTORY_KEEP_ALL = 1,
}

/// QoS settings structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct nano_ros_qos_t {
    /// Reliability policy
    pub reliability: nano_ros_qos_reliability_t,
    /// Durability policy
    pub durability: nano_ros_qos_durability_t,
    /// History policy
    pub history: nano_ros_qos_history_t,
    /// History depth (for KEEP_LAST)
    pub depth: c_int,
}

impl Default for nano_ros_qos_t {
    fn default() -> Self {
        Self {
            reliability: nano_ros_qos_reliability_t::NANO_ROS_QOS_RELIABILITY_RELIABLE,
            durability: nano_ros_qos_durability_t::NANO_ROS_QOS_DURABILITY_VOLATILE,
            history: nano_ros_qos_history_t::NANO_ROS_QOS_HISTORY_KEEP_LAST,
            depth: 10,
        }
    }
}

/// Default QoS profile
#[unsafe(no_mangle)]
pub static NANO_ROS_QOS_DEFAULT: nano_ros_qos_t = nano_ros_qos_t {
    reliability: nano_ros_qos_reliability_t::NANO_ROS_QOS_RELIABILITY_RELIABLE,
    durability: nano_ros_qos_durability_t::NANO_ROS_QOS_DURABILITY_VOLATILE,
    history: nano_ros_qos_history_t::NANO_ROS_QOS_HISTORY_KEEP_LAST,
    depth: 10,
};

/// Sensor data QoS profile (best effort, small depth)
#[unsafe(no_mangle)]
pub static NANO_ROS_QOS_SENSOR_DATA: nano_ros_qos_t = nano_ros_qos_t {
    reliability: nano_ros_qos_reliability_t::NANO_ROS_QOS_RELIABILITY_BEST_EFFORT,
    durability: nano_ros_qos_durability_t::NANO_ROS_QOS_DURABILITY_VOLATILE,
    history: nano_ros_qos_history_t::NANO_ROS_QOS_HISTORY_KEEP_LAST,
    depth: 5,
};

/// Services QoS profile (reliable)
#[unsafe(no_mangle)]
pub static NANO_ROS_QOS_SERVICES: nano_ros_qos_t = nano_ros_qos_t {
    reliability: nano_ros_qos_reliability_t::NANO_ROS_QOS_RELIABILITY_RELIABLE,
    durability: nano_ros_qos_durability_t::NANO_ROS_QOS_DURABILITY_VOLATILE,
    history: nano_ros_qos_history_t::NANO_ROS_QOS_HISTORY_KEEP_LAST,
    depth: 10,
};

impl nano_ros_qos_t {
    /// Convert to nros QosSettings
    pub(crate) fn to_qos_settings(self) -> nros_rmw::QosSettings {
        use nros_rmw::{QosDurabilityPolicy, QosHistoryPolicy, QosReliabilityPolicy};

        let reliability = match self.reliability {
            nano_ros_qos_reliability_t::NANO_ROS_QOS_RELIABILITY_BEST_EFFORT => {
                QosReliabilityPolicy::BestEffort
            }
            nano_ros_qos_reliability_t::NANO_ROS_QOS_RELIABILITY_RELIABLE => {
                QosReliabilityPolicy::Reliable
            }
        };

        let durability = match self.durability {
            nano_ros_qos_durability_t::NANO_ROS_QOS_DURABILITY_VOLATILE => {
                QosDurabilityPolicy::Volatile
            }
            nano_ros_qos_durability_t::NANO_ROS_QOS_DURABILITY_TRANSIENT_LOCAL => {
                QosDurabilityPolicy::TransientLocal
            }
        };

        let history = match self.history {
            nano_ros_qos_history_t::NANO_ROS_QOS_HISTORY_KEEP_LAST => QosHistoryPolicy::KeepLast,
            nano_ros_qos_history_t::NANO_ROS_QOS_HISTORY_KEEP_ALL => QosHistoryPolicy::KeepAll,
        };

        nros_rmw::QosSettings {
            reliability,
            durability,
            history,
            depth: self.depth as u32,
        }
    }
}
