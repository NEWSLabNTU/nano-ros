/// @file Listener.cpp
/// @brief C++ Listener component — Phase 212.L Component pkg.
///
/// Subscribes to `std_msgs/Int32` on `/chatter`. The generated runtime
/// owns init / executor / spin; this file declares the component class
/// + exports the register trampoline via `NROS_NODE_REGISTER`.

#include <cstdint>

#include <nros/node_pkg.hpp>
#include <nros/nros.hpp>
#include "std_msgs.hpp"

namespace threadx_linux_cpp_listener {

class Listener {
  public:
    static nros::Result register_node(nros::NodeContext& context) {
        nros::DeclaredNode node;
        nros::NodeOptions options;
        options.name = "listener";
        options.namespace_ = "/";
        nros::Result rc = context.create_node(node, options);
        if (!rc.ok()) return rc;

        nros::DeclaredCallback on_chatter;
        rc = node.declare_callback(on_chatter, "on_chatter");
        if (!rc.ok()) return rc;

        nros::DeclaredEntity sub;
        return node.create_subscription(sub, "/chatter", "std_msgs/msg/Int32", on_chatter);
    }
};

} // namespace threadx_linux_cpp_listener

NROS_NODE_REGISTER(threadx_linux_cpp_listener::Listener, "threadx_linux_cpp_listener::Listener");
