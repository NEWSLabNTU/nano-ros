//! Node construction options — rclrs's `NodeOptions` / `IntoNodeOptions`
//! (phase-515 R2).
//!
//! Lives here rather than in `nros` so the executor's own
//! [`create_node`](crate::Executor::create_node) can take it; `nros`
//! re-exports both names unchanged.

/// Runtime-neutral node construction options.
///
/// ADOPT-BOUNDED against rclrs's `NodeOptions` / `IntoNodeOptions` builder: three
/// public fields (`name`, `namespace`, `domain_id`) and nothing else. There are no
/// node ARGUMENTS and no rosout toggle — both are fixed at build time here
/// (RFC-0045), so an rclrs builder call for either is a compile error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeOptions<'a> {
    /// Source node name. Launch planning may remap/namespace later.
    pub name: &'a str,
    /// Source namespace. Defaults to `/`.
    pub namespace: &'a str,
    /// ROS domain ID hint. Defaults to `0`.
    pub domain_id: u32,
}

impl<'a> NodeOptions<'a> {
    /// Create node options with default namespace and domain.
    pub const fn new(name: &'a str) -> Self {
        Self {
            name,
            namespace: "/",
            domain_id: 0,
        }
    }

    /// Set source namespace.
    pub const fn namespace(mut self, namespace: &'a str) -> Self {
        self.namespace = namespace;
        self
    }

    /// Set ROS domain ID hint.
    pub const fn domain_id(mut self, domain_id: u32) -> Self {
        self.domain_id = domain_id;
        self
    }
}

/// rclrs's `IntoNodeOptions`: anything a node can be created from — a bare
/// name (`"talker"`) or a full [`NodeOptions`] — so
/// `executor.create_node("talker")` and
/// `executor.create_node("talker".namespace("/robot"))` read as they do in
/// rclrs.
///
/// ADOPT-BOUNDED: rclrs's other builder steps (`arguments`,
/// `use_global_arguments`, `enable_rosout`, `start_parameter_services`,
/// `clock_type`, `clock_qos`) configure what is fixed at build time here
/// (RFC-0045), so they are absent and a call to one is a compile error.
pub trait IntoNodeOptions<'a>: Sized {
    /// Convert to the full options value.
    fn into_node_options(self) -> NodeOptions<'a>;

    /// Set the namespace — rclrs's `"name".namespace("/ns")`.
    fn namespace(self, namespace: &'a str) -> NodeOptions<'a> {
        self.into_node_options().namespace(namespace)
    }
}

impl<'a> IntoNodeOptions<'a> for &'a str {
    fn into_node_options(self) -> NodeOptions<'a> {
        NodeOptions::new(self)
    }
}

impl<'a> IntoNodeOptions<'a> for NodeOptions<'a> {
    fn into_node_options(self) -> NodeOptions<'a> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_name_takes_the_defaults() {
        assert_eq!("talker".into_node_options(), NodeOptions::new("talker"));
    }

    #[test]
    fn rclrs_namespace_step_on_a_bare_name() {
        let o = "talker".namespace("/robot");
        assert_eq!(o.name, "talker");
        assert_eq!(o.namespace, "/robot");
        assert_eq!(o.domain_id, 0);
    }

    #[test]
    fn full_options_pass_through() {
        let o = NodeOptions::new("talker").domain_id(7);
        assert_eq!(o.into_node_options(), o);
    }
}
