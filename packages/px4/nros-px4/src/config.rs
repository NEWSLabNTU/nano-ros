//! Configuration for [`crate::run`] / [`crate::run_async`].

use core::time::Duration;

/// Per-module configuration. Mirrors the shape of other board crates
/// (`nros-mps2-an385::Config`, `nros-nuttx-qemu-arm::Config`).
#[derive(Debug, Clone, Copy)]
pub struct Config<'a> {
    /// PX4 WorkQueue name the executor binds to (e.g. `"rate_ctrl"`,
    /// `"hp_default"`, `"lp_default"`). Must match a `px4_workqueue::wq_configurations::*`
    /// canonical name.
    pub wq_name: &'a str,
    /// ROS 2 node name reported via parameter services and liveliness.
    pub node_name: &'a str,
    /// ROS 2 namespace (default `""`).
    pub namespace: &'a str,
    /// ROS domain id. Currently unused by uORB (in-process); kept for API
    /// compatibility with other board crates.
    pub domain_id: u32,
    /// Maximum park duration for [`crate::run_async`] when no uORB
    /// topic is pending and the executor has no work to drain. Bounds
    /// the latency of nros timers and any wake source not routed
    /// through the uORB callback chain (currently `GuardCondition`).
    /// Default 50 ms.
    ///
    /// Pick higher for timer-light apps to lower idle CPU; pick lower
    /// to tighten timer accuracy. Has no effect on
    /// uORB-publish-driven latency — those wake the executor via
    /// `ScheduleNow` regardless.
    pub park_max: Duration,
}

impl<'a> Config<'a> {
    /// Convenience constructor with sensible defaults for the optional fields.
    pub const fn new(wq_name: &'a str, node_name: &'a str) -> Self {
        Self {
            wq_name,
            node_name,
            namespace: "",
            domain_id: 0,
            park_max: Duration::from_millis(50),
        }
    }

    /// Override [`Self::park_max`].
    pub const fn with_park_max(mut self, park_max: Duration) -> Self {
        self.park_max = park_max;
        self
    }
}
