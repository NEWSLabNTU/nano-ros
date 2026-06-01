/// @file Listener.cpp
/// @brief C++ Listener component — Phase 212.L Component pkg.
///
/// Subscribes to `std_msgs/Int32` on `/chatter`. The generated runtime
/// owns init / executor / spin; this file declares the component class
/// + exports the register trampoline via `NROS_COMPONENT_REGISTER`.

#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>
#include "std_msgs.hpp"

namespace threadx_linux_cpp_listener {

class Listener {
  public:
    static nros::Result register_component(nros::ComponentContext& context) {
        nros::ComponentNode node;
        nros::NodeOptions options;
        options.name = "listener";
        options.namespace_ = "/";
        nros::Result rc = context.create_node(node, "node", options);
        if (!rc.ok()) return rc;

        nros::ComponentEntityDescriptor sub{};
        sub.id = "sub_chatter";
        sub.kind = nros::EntityKind::Subscription;
        sub.source_name = "/chatter";
        sub.type_name = std_msgs::msg::Int32::TYPE_NAME;
        sub.type_hash = std_msgs::msg::Int32::TYPE_HASH;
        sub.callback_id = "on_chatter";
        return node.create_entity(sub);
    }
};

} // namespace threadx_linux_cpp_listener

NROS_COMPONENT_REGISTER(threadx_linux_cpp_listener::Listener,
                        "threadx_linux_cpp_listener::Listener");
