// AddServer — typed AddTwoInts service handler bound by identity (no CDR by hand). The ROS 2
// tutorial `add_two_ints_server`: prints `Incoming request` and the operands for each call.
// A copy of `examples/workspaces/cpp`'s component of the same role: a bare-metal image needs its
// own configure (issue 2555).

#include "baremetal_cpp_service_server/AddServer.hpp"

#include <cstdio>

namespace baremetal_cpp_service_server {

example_interfaces::srv::AddTwoInts::Response
AddServer::on_request(const example_interfaces::srv::AddTwoInts::Request& req) {
    example_interfaces::srv::AddTwoInts::Response resp;
    resp.sum = req.a + req.b;
    std::printf("Incoming request\na: %ld b: %ld\n", static_cast<long>(req.a),
                static_cast<long>(req.b));
    return resp;
}

::rclcpp::Result AddServer::configure(::rclcpp::Node& node) {
    // `::setvbuf` (C global), not `std::setvbuf` — Zephyr picolibc lacks the std:: name.
    ::setvbuf(stdout, nullptr, _IONBF, 0);
    ::rclcpp::Result r =
        ::rclcpp::bind_service<Svc, AddServer, &AddServer::on_request>(node, "/add_two_ints", this);
    if (r.ok()) {
        std::printf("Waiting for service requests\n");
    }
    return r;
}

} // namespace baremetal_cpp_service_server
