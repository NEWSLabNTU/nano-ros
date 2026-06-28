#pragma once

#include <nros/component.hpp>
#include <nros/nros.hpp>

#include "example_interfaces.hpp"

namespace cpp_add_server_pkg {

/// AddServer — A1 services (C++ projection of the Rust `service_server_pkg`). A typed component:
/// `configure` binds the member `on_request` as the `/add_two_ints` service handler via the
/// TYPED `::nros::bind_service<Svc, C, &method>` (issue 0089 gap 4 — the trampoline
/// ffi_deserializes the request + ffi_serializes the response; no hand-rolled CDR). Cross-process
/// only (issue 0096): server + client run as two entries.
class AddServer {
    using Svc = example_interfaces::srv::AddTwoInts;

    Svc::Response on_request(const Svc::Request& req);

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace cpp_add_server_pkg
