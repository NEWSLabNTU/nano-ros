// AddCallbackClient — the callback-receive AddTwoInts client (RFC-0041): the reply is printed
// from inside the response callback, so its line proves the callback dispatched.

#include "service_client_callback_pkg/AddCallbackClient.hpp"

#include <cstdio>

namespace service_client_callback_pkg {

namespace {
/// Bumped by the response callback. The callback type is a plain function pointer (no context),
/// and an image holds one instance of this component.
int g_replies = 0;

void on_response(const example_interfaces::srv::AddTwoInts::Response& resp) {
    g_replies++;
    std::printf("Result of add_two_ints: %ld\n", static_cast<long>(resp.sum));
}
} // namespace

void AddCallbackClient::on_tick() {
    if (g_replies > 0) {
        return;
    }
    if (waits_ > 0 && waits_ <= 6) {
        waits_++;
        return;
    }
    Svc::Request req;
    req.a = kA;
    req.b = kB;
    if (client_.async_send_request(req).ok()) {
        waits_ = 1;
    }
}

::rclcpp::Result AddCallbackClient::configure(::rclcpp::Node& node) {
    ::setvbuf(stdout, nullptr, _IONBF, 0);
    ::rclcpp::Result r = node.create_client(client_, "/add_two_ints", &on_response);
    if (!r.ok()) {
        return r;
    }
    return node.create_wall_timer<AddCallbackClient, &AddCallbackClient::on_tick>(timer_, 500,
                                                                                  this);
}

} // namespace service_client_callback_pkg
