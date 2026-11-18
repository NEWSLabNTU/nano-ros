#pragma once

#include <rclcpp/rclcpp.hpp>

namespace seeds_cpp_pkg {

/// Issue 2640 -- a C++ component whose period has no default in code.
///
/// `ctrl_period_ms` is declared with `declare_parameter<int64_t>(name)`, which
/// reads `T()` when nothing seeded the name; the timer is created only for a
/// positive period. The 40 `gain_NN` parameters are more than a default
/// parameter store holds (32).
class Controller : public ::rclcpp::NodeWithTimers<1> {
    void on_timer() {}

  public:
    explicit Controller(::rclcpp::NodeHandle h);
};

} // namespace seeds_cpp_pkg
