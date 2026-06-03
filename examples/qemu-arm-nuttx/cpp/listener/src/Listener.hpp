// Phase 212.L Component pkg — NuttX C++ listener.
#ifndef NUTTX_CPP_LISTENER_LISTENER_HPP
#define NUTTX_CPP_LISTENER_LISTENER_HPP

#include <nros/node_pkg.hpp>

namespace nuttx_cpp_listener {

class Listener {
  public:
    static ::nros::Result register_component(::nros::ComponentContext& context);
};

} // namespace nuttx_cpp_listener

#endif // NUTTX_CPP_LISTENER_LISTENER_HPP
