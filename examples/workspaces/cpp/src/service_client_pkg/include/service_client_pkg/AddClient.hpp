#pragma once

#include <cstdint>

#include <rclcpp/rclcpp.hpp>

#include "example_interfaces.hpp"

namespace service_client_pkg {

/// AddClient — the ROS 2 tutorial service client (demo_nodes_cpp `add_two_ints_client`) as a
/// typed component: ONE request, a = 2 and b = 3, and one line, `Result of add_two_ints: 5`.
/// phase-535 made this the ONE C++ client: the standalone example pairs and the workspace images
/// run the same program (RFC-0122 D1, option B).
///
/// A component callback must never block the executor, so the call is not the tutorial's
/// blocking one: a 500 ms timer drives the POLL-model client (`create_service_client_raw` + the
/// raw send/take FFI), sending the request and polling for its reply. A wait counter re-sends if
/// a reply never arrives (the first request can be dropped before the server is discovered).
/// After the reply the timer only idles.
class AddClient {
    using Svc = example_interfaces::srv::AddTwoInts;

    ::rclcpp::ServiceClientStorage client_;
    ::rclcpp::Timer timer_;
    /// The tutorial's request: 2 + 3.
    static constexpr int64_t kA = 2;
    static constexpr int64_t kB = 3;
    bool in_flight_ = false;
    bool done_ = false; ///< the reply arrived and was printed
    int waits_ = 0;
    /// The id of the request in flight; a reply naming another is stale (issue 2033).
    int64_t seq_ = NROS_CPP_SEQUENCE_ID_UNKNOWN;

    void on_tick();

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

} // namespace service_client_pkg
