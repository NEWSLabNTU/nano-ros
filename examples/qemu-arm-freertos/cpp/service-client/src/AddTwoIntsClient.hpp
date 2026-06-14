// FreeRTOS C++ AddTwoInts service client — typed poll component (RFC-0043, 240.5).
//
// `configure` creates a service client + a timer that polls: each cycle sends a
// request (a, b) and, on the next ticks, polls the reply and prints the sum.
// (Poll model — clients move to callbacks when RFC-0041's C/C++ wave lands.)
#ifndef FREERTOS_CPP_SERVICE_CLIENT_ADDTWOINTSCLIENT_HPP
#define FREERTOS_CPP_SERVICE_CLIENT_ADDTWOINTSCLIENT_HPP

#include <cstddef>
#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>

namespace freertos_cpp_service_client {

class AddTwoIntsClient {
    ::nros::ServiceClientStorage client_;
    ::nros::Timer timer_;
    int64_t a_ = 1;
    int64_t b_ = 2;
    bool awaiting_ = false;

    void on_tick(); // send / poll driver, bound by identity

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace freertos_cpp_service_client

#endif
