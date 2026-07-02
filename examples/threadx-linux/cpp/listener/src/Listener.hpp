// ThreadX-Linux C++ listener — typed component (RFC-0043). `configure` binds the
// member `on_raw` (by identity, no callback name) as a raw zero-copy subscription
// on `/chatter`; the real executor dispatches it each `spin_once`. Platform/RMW
// selection lives in CMake, not here.
#ifndef THREADX_LINUX_CPP_LISTENER_LISTENER_HPP
#define THREADX_LINUX_CPP_LISTENER_LISTENER_HPP

#include <cstddef>
#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

namespace threadx_linux_cpp_listener {

class Listener {
    int recv_ = 0;

    void on_raw(const uint8_t* data, size_t len); // real body; bound by identity

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace threadx_linux_cpp_listener

#endif // THREADX_LINUX_CPP_LISTENER_LISTENER_HPP
