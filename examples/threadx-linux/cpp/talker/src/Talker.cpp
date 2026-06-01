/// @file Talker.cpp
/// @brief C++ Talker component — Phase 212.L Component pkg.
///
/// Publishes `std_msgs/Int32` on `/chatter`. The generated runtime
/// (emitted by `nros codegen-system` via the H.4 ThreadX adapter) owns
/// init / executor / spin; this file declares the component class +
/// exports the register trampoline via `NROS_COMPONENT_REGISTER`.

#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>
#include "std_msgs.hpp"

namespace threadx_linux_cpp_talker {

class Talker {
  public:
    static nros::Result register_component(nros::ComponentContext& context) {
        nros::ComponentNode node;
        nros::NodeOptions options;
        options.name = "talker";
        options.namespace_ = "/";
        nros::Result rc = context.create_node(node, "node", options);
        if (!rc.ok()) return rc;

        nros::ComponentEntityDescriptor pub{};
        pub.id = "pub_chatter";
        pub.kind = nros::EntityKind::Publisher;
        pub.source_name = "/chatter";
        pub.type_name = std_msgs::msg::Int32::TYPE_NAME;
        pub.type_hash = std_msgs::msg::Int32::TYPE_HASH;
        return node.create_entity(pub);
    }
};

} // namespace threadx_linux_cpp_talker

NROS_COMPONENT_REGISTER(threadx_linux_cpp_talker::Talker,
                        "threadx_linux_cpp_talker::Talker");
