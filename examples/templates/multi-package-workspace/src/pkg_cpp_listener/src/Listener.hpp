// multi-package-workspace demo — C++ listener, typed component
// (RFC-0043 / phase-244.C4). A stateful component: `configure` binds the member
// `on_raw` (by identity, no callback name) as a raw zero-copy subscription on
// `/chatter`. The native typed Entry carrier constructs this object + calls
// `configure(node)` and runs `NativeBoard::run_components`.
#ifndef PKG_CPP_LISTENER_LISTENER_HPP
#define PKG_CPP_LISTENER_LISTENER_HPP

#include <cstddef>
#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>

namespace pkg_cpp_listener {

class Listener {
    int recv_ = 0;

    void on_raw(const uint8_t* data, size_t len); // real body; bound by identity

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace pkg_cpp_listener

#endif // PKG_CPP_LISTENER_LISTENER_HPP
