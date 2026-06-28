// AddServer — typed AddTwoInts service handler bound by identity (no CDR by hand). The C++
// projection of the Rust service_server_pkg / the C c_add_server_pkg.

#include "cpp_add_server_pkg/AddServer.hpp"

#include <cstdio>

namespace cpp_add_server_pkg {

example_interfaces::srv::AddTwoInts::Response
AddServer::on_request(const example_interfaces::srv::AddTwoInts::Request& req) {
    example_interfaces::srv::AddTwoInts::Response resp;
    resp.sum = req.a + req.b;
    std::printf("[cpp_add_server_pkg] %lld + %lld = %lld\n", static_cast<long long>(req.a),
                static_cast<long long>(req.b), static_cast<long long>(resp.sum));
    return resp;
}

::nros::Result AddServer::configure(::nros::Node& node) {
    // `::setvbuf` (C global), not `std::setvbuf` — Zephyr picolibc lacks the std:: name.
    ::setvbuf(stdout, nullptr, _IONBF, 0);
    ::nros::Result r =
        ::nros::bind_service<Svc, AddServer, &AddServer::on_request>(node, "/add_two_ints", this);
    if (r.ok()) {
        std::printf("[cpp_add_server_pkg] add_two_ints server ready\n");
    }
    return r;
}

} // namespace cpp_add_server_pkg
