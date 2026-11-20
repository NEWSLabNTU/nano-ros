#pragma once

#include <rclcpp/rclcpp.hpp>

namespace gq_cpp_pkg {

/// The C++ half of the graph-query parity probe. Every 100 ms it asks the
/// node for the graph's node, topic and service counts and for `/chatter`'s
/// publisher count and type. Once `/chatter` has a publisher and the answer
/// has held for five probes, it prints the answer once.
class Probe : public ::rclcpp::NodeWithTimers<1> {
    char last_[256] = "";
    int stable_ = 0;
    bool done_ = false;
    void on_timer();

  public:
    explicit Probe(::rclcpp::NodeHandle h);
};

} // namespace gq_cpp_pkg
