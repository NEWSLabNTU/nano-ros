/// @file main.cpp
/// @brief Phase 240.1 (RFC-0043) proof — stateful component objects bind real
/// member callbacks (timer publish + raw zero-copy subscription) and run through
/// the real executor. No declarative string descriptors, no synthesizing
/// interpreter, no callback names.
///
/// Run both roles in one process (default — relies on the RMW's local delivery)
/// or split: `component_poc talker` / `component_poc listener`.

#include <cstdio>
#include <cstdlib>
#include <cstring>

#include <nros/component.hpp>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

using Int32 = std_msgs::msg::Int32;

// ---- Talker: a timer member callback publishes a real counter --------------
class Talker {
    nros::Publisher<Int32> pub_;
    nros::Timer timer_;
    int count_ = 0;

    void on_tick() { // real body, bound by identity (no name)
        Int32 m;
        m.data = count_++;
        if (pub_.publish(m).ok()) {
            std::printf("Published: %d\n", m.data);
        }
    }

  public:
    nros::Result configure(nros::Node& node) {
        nros::Result r = node.create_publisher(pub_, "/chatter");
        if (!r.ok()) return r;
        return nros::bind_timer<Talker, &Talker::on_tick>(node, timer_, 500, this);
    }
};

// ---- Listener: a raw (zero-copy) sub member callback counts receipts --------
class Listener {
    int recv_ = 0;

    void on_raw(const uint8_t* data, size_t len) { // real body, bound by identity
        int32_t v = 0;
        if (len >= 8) {
            v = static_cast<int32_t>(
                static_cast<uint32_t>(data[4]) | (static_cast<uint32_t>(data[5]) << 8) |
                (static_cast<uint32_t>(data[6]) << 16) | (static_cast<uint32_t>(data[7]) << 24));
        }
        std::printf("Received: %d\n", v);
        ++recv_;
    }

  public:
    nros::Result configure(nros::Node& node) {
        // NB: the wire keyexpr uses the DDS-mangled type name the typed
        // `Publisher<Int32>` registers (`std_msgs::msg::dds_::Int32_`), not the
        // ROS slash form — so the raw sub must pass `Int32::TYPE_NAME` to match.
        // (Raw-vs-typed type-name-form unification is a separate concern.)
        return nros::bind_subscription_raw<Listener, &Listener::on_raw>(node, "/chatter",
                                                                        Int32::TYPE_NAME, this);
    }
};

int main(int argc, char** argv) {
    const char* role = (argc > 1) ? argv[1] : "both";
    std::printf("component-poc role=%s\n", role);

    if (!nros::init().ok()) {
        std::printf("init failed\n");
        return 1;
    }

    nros::Node node;
    if (!nros::create_node(node, "component_poc").ok()) {
        std::printf("create_node failed\n");
        return 1;
    }

    // Components are stack-owned here (must outlive the spin loop — the executor
    // holds &member as dispatch context). The codegen Entry (240.2) will own
    // them in an arena slot.
    Talker talker;
    Listener listener;

    if (std::strcmp(role, "listener") != 0) {
        if (!talker.configure(node).ok()) {
            std::printf("talker.configure failed\n");
            return 1;
        }
    }
    if (std::strcmp(role, "talker") != 0) {
        if (!listener.configure(node).ok()) {
            std::printf("listener.configure failed\n");
            return 1;
        }
        std::printf("Waiting for messages\n");
    }

    for (int i = 0; i < 200 && nros::ok(); ++i) {
        nros::spin_once(50); // executor dispatches the real member callbacks
    }

    nros::shutdown();
    std::printf("done\n");
    return 0;
}
