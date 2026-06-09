/// @file Talker.cpp
/// @brief C++ Talker component — Phase 212.L Component pkg.
///
/// Publishes `std_msgs/Int32` on `/chatter`. The generated runtime
/// (emitted by `nros codegen-system` via the H.4 ThreadX adapter) owns
/// init / executor / spin; this file declares the component class +
/// exports the register trampoline via `NROS_NODE_REGISTER`.

#include <cstdint>

#include <nros/nros.hpp>
#include <nros/node_pkg.hpp>
#include "std_msgs.hpp"

namespace threadx_linux_cpp_talker {

class Talker {
  public:
    static nros::Result register_node(nros::NodeContext& context) {
        nros::DeclaredNode node;
        nros::NodeOptions options;
        options.name = "talker";
        options.namespace_ = "/";
        nros::Result rc = context.create_node(node, options);
        if (!rc.ok()) return rc;

        nros::DeclaredEntity pub;
        return node.create_publisher(pub, "/chatter", "std_msgs/msg/Int32");
    }
};

} // namespace threadx_linux_cpp_talker

NROS_NODE_REGISTER(threadx_linux_cpp_talker::Talker, "threadx_linux_cpp_talker::Talker");
