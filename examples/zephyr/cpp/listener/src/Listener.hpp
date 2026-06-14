// Zephyr C++ listener — typed component (RFC-0043 / phase-244.C2). A stateful
// component object: `configure` binds the member `on_raw` (by identity, no
// callback name) as a raw zero-copy subscription on `/chatter`. The Zephyr typed
// carrier (`zephyr_entry_main_typed.cpp.in`) constructs this object + calls
// `configure(node)` and runs `ZephyrBoard::run_components`.
#ifndef NROS_ZEPHYR_LISTENER_CPP_LISTENER_HPP
#define NROS_ZEPHYR_LISTENER_CPP_LISTENER_HPP

#include <cstddef>
#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>

namespace nros_zephyr_listener_cpp {

class Listener {
    int recv_ = 0;

    void on_raw(const uint8_t* data, size_t len); // real body; bound by identity

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace nros_zephyr_listener_cpp

#endif // NROS_ZEPHYR_LISTENER_CPP_LISTENER_HPP
