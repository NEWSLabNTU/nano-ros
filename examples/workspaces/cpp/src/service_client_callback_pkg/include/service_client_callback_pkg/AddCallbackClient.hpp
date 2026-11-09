#pragma once

#include <cstdint>

#include <rclcpp/rclcpp.hpp>

#include "example_interfaces.hpp"

namespace service_client_callback_pkg {

/// AddCallbackClient — the tutorial AddTwoInts client (2 + 3, one line
/// `Result of add_two_ints: 5`), **callback** variant (RFC-0041): the reply arrives through the
/// response handler `node.create_client(client, name, cb)` registers in the executor arena, which
/// `spin_once` dispatches. No Future, no poll. phase-535 W6 (RFC-0122 D2) made the old
/// `examples/linux/cpp/service-client-callback` leaf this node package.
///
/// A component callback must never block the executor, so a 500 ms timer sends the request with
/// `async_send_request` and re-sends while no reply has arrived (the first request can be dropped
/// before the server is discovered). After the reply the timer only idles.
class AddCallbackClient {
    using Svc = example_interfaces::srv::AddTwoInts;

    ::rclcpp::Client<Svc> client_;
    ::rclcpp::Timer timer_;
    /// The tutorial's request: 2 + 3.
    static constexpr int64_t kA = 2;
    static constexpr int64_t kB = 3;
    int waits_ = 0; ///< ticks waited for the current reply (resend guard)

    void on_tick();

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

} // namespace service_client_callback_pkg
