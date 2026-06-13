// ThreadX-Linux C++ AddTwoInts service client — typed poll component (RFC-0043).
// `configure` creates a service client + a timer that polls: each cycle sends a
// request (a, b) and polls the reply, printing the sum. (Poll model — clients move
// to callbacks when RFC-0041's C/C++ wave lands.) No callback name.
#ifndef THREADX_LINUX_CPP_SERVICE_CLIENT_SERVICECLIENT_HPP
#define THREADX_LINUX_CPP_SERVICE_CLIENT_SERVICECLIENT_HPP

#include <cstddef>
#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>

namespace threadx_linux_cpp_service_client {

class ServiceClient {
    ::nros::ServiceClientStorage client_;
    ::nros::Timer timer_;
    int64_t a_ = 1;
    int64_t b_ = 2;
    bool awaiting_ = false;

    void on_tick(); // send / poll driver, bound by identity

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace threadx_linux_cpp_service_client

#endif
