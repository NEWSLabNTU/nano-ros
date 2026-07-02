// ThreadX-Linux C++ AddTwoInts service client — typed poll component (RFC-0043).
// `configure` creates a service client + a timer that polls: the first tick sends
// ONE fixed request (2, 3); later ticks poll the reply and print the sum, then
// the client goes quiet. (Poll model — clients move to callbacks when RFC-0041's
// C/C++ wave lands.) No callback name.
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
    int64_t a_ = 2;
    int64_t b_ = 3;
    bool awaiting_ = false;
    bool done_ = false;

    void on_tick(); // send / poll driver, bound by identity

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace threadx_linux_cpp_service_client

#endif
