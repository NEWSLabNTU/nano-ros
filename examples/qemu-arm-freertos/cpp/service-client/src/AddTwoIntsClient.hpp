// FreeRTOS C++ AddTwoInts service client — typed poll component.
//
// `configure` creates a service client + a timer that polls: the first tick
// sends one fixed request (2, 3); later ticks poll the reply and print the
// sum, then go idle. (Poll model — the C/C++ client API is poll-based.)
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
    // Embedded client: one fixed request (2, 3) — no argv on firmware.
    int64_t a_ = 2;
    int64_t b_ = 3;
    bool awaiting_ = false;
    bool done_ = false;

    void on_tick(); // send / poll driver, bound by identity

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace freertos_cpp_service_client

#endif
