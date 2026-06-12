#include <nros/node_pkg.hpp>

struct Msg {
    static constexpr const char* TYPE_NAME = "std_msgs/msg/Int32";
    static constexpr const char* TYPE_HASH = "";
};

int main() {
    nros::DeclaredNode node;
    (void)node.create_publisher<Msg>("chatter", nros::QoS::default_profile());
    (void)node.create_subscription<Msg>("/chatter", "on_message", nros::QoS::default_profile());
    return 0;
}
