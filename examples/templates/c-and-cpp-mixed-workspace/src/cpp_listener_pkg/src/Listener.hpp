#pragma once

#include <nros/node_pkg.hpp>

namespace cpp_listener_pkg {

class Listener {
  public:
    static ::nros::Result register_node(::nros::NodeContext& ctx);
};

} // namespace cpp_listener_pkg
