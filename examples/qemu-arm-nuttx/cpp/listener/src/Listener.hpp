// NuttX C++ listener — typed component (RFC-0043, phase-240.3).
//
// A stateful component object: `configure` binds the member `on_raw` (by
// identity, no callback name) as a raw zero-copy subscription on `/chatter`.
// The typed Entry carrier constructs this object + calls `configure(node)`;
// the real executor dispatches `on_raw` during `spin_once`.
#ifndef NUTTX_CPP_LISTENER_LISTENER_HPP
#define NUTTX_CPP_LISTENER_LISTENER_HPP

#include <cstddef>
#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

namespace nuttx_cpp_listener {

class Listener {
    int recv_ = 0;

    void on_raw(const uint8_t* data, size_t len); // real body; bound by identity

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace nuttx_cpp_listener

#endif // NUTTX_CPP_LISTENER_LISTENER_HPP
