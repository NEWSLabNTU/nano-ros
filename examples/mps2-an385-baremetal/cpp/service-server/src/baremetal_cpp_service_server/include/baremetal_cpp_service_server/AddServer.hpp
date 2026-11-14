#pragma once

#include <rclcpp/rclcpp.hpp>

#include "example_interfaces.hpp"

namespace baremetal_cpp_service_server {

/// AddServer — A1 services (C++ projection of the Rust `baremetal_cpp_service_server`). A typed
/// component: `configure` binds the member `on_request` as the `/add_two_ints` service handler via
/// the TYPED `::rclcpp::bind_service<Svc, C, &method>` (issue 0089 gap 4 — the trampoline
/// ffi_deserializes the request + ffi_serializes the response; no hand-rolled CDR). Cross-process
/// only (issue 0096): server + client run as two entries.
class AddServer {
    using Svc = example_interfaces::srv::AddTwoInts;

    Svc::Response on_request(const Svc::Request& req);

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

} // namespace baremetal_cpp_service_server
