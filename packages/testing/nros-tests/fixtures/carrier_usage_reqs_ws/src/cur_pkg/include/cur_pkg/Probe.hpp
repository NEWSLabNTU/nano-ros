#pragma once

#include <rclcpp/rclcpp.hpp>

// From `node/include`, a PUBLIC include directory of cur_lib that is not this
// package's include/ or src/.
#include <cur_pkg_node/probe_period.hpp>

namespace cur_pkg {

/// Prints one `CARRIER_USAGE_REQS answer=<n>` line, `<n>` from cur_vendor.
class Probe : public ::rclcpp::NodeWithTimers<1> {
    bool done_ = false;
    void on_timer();

  public:
    explicit Probe(::rclcpp::NodeHandle h);
};

} // namespace cur_pkg
