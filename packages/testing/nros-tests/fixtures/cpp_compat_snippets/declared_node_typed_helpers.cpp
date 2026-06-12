// Issue #34 — typed (`<M>`) DeclaredNode helper compile-check. Updated to the
// current API (Phase 220.B): the typed `create_publisher`/`create_subscription`
// take an out `DeclaredEntity&` first, and the subscription takes a
// `DeclaredCallback&` (built via `declare_callback`) rather than a callback-id
// string. The old `(topic, callback_id, qos)` form only matched the untyped
// overload, so the `<Msg>` call no longer compiled — this snippet is the drift
// guard that pins the typed surface.
#include <nros/node_pkg.hpp>

struct Msg {
    static constexpr const char* TYPE_NAME = "std_msgs/msg/Int32";
    static constexpr const char* TYPE_HASH = "";
};

int main() {
    nros::DeclaredNode node;

    nros::DeclaredEntity pub;
    (void)node.create_publisher<Msg>(pub, "chatter", nros::QoS::default_profile());

    nros::DeclaredCallback cb;
    (void)node.declare_callback(cb, "on_message");

    nros::DeclaredEntity sub;
    (void)node.create_subscription<Msg>(sub, "/chatter", cb, nros::QoS::default_profile());
    return 0;
}
