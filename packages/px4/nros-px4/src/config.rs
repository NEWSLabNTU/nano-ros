//! Configuration for [`crate::run`].

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
}

impl<'a> Config<'a> {
    /// Convenience constructor with sensible defaults for the optional fields.
    pub const fn new(wq_name: &'a str, node_name: &'a str) -> Self {
        Self {
            wq_name,
            node_name,
            namespace: "",
            domain_id: 0,
        }
    }
}
