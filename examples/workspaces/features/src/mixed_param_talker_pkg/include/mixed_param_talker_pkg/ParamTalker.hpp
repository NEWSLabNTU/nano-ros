#pragma once

#include <nros/component.hpp>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

namespace mixed_param_talker_pkg {

/// ParamTalker — phase-426 W5's acceptance: one node, two languages, ONE
/// parameter store.
///
/// The component is C++; `src/param_probe.c` is a C translation unit in the
/// same package, handed this node's `nros_cpp_node_t*`. Each language DECLARES
/// one parameter and READS the other's:
///
///   * C++ declares `publish_period_ms` (adopting the launch `<param>` seed)
///     and reads `scale`, which C declared.
///   * C declares `scale` and reads `publish_period_ms`, which C++ declared.
///
/// The tick publishes `publish_period_ms * scale` on /chatter, read through C
/// on every tick — so the number on the wire is only correct if both crossings
/// land in the same `nros_params::ParameterServer`. A second store anywhere in
/// the chain publishes a default instead.
class ParamTalker {
    ::rclcpp::Publisher<std_msgs::msg::Int32> pub_;
    ::nros::Timer timer_;
    /// Saved at configure: the C half is a free function over this handle, and
    /// the parameter store is keyed by NODE (phase-426 W1), so naming the node
    /// is what makes "the other language's parameter" resolvable at all.
    const nros_cpp_node_t* node_handle_ = nullptr;
    double scale_ = 0.0;

    void on_tick();

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

} // namespace mixed_param_talker_pkg
