// phase-427 W2 acceptance — the FREESTANDING half of the merged node type.
//
// Compiled `-nostdinc++` against the ThreadX minimal libcpp (the one lane in
// this tree that can see a freestanding regression), so nothing here may reach
// `<memory>`, `<string>`, `<vector>`, `<functional>` or `<chrono>`. The point
// is not that the headers PARSE — the header lane already checks that — but
// that a TU which CONSTRUCTS a node and CREATES A PUBLISHER type-checks all the
// way through the template bodies on a target with no allocator.
//
// This is the probe the merge could break most quietly. Merging the hosted
// `rclcpp::Node` into the one type puts `std::shared_ptr` signatures, a
// `std::vector` of owned cells and a `NodeOptions` on the same class a
// freestanding image instantiates; if any of that leaks out of
// `NROS_CPP_NODE_HOSTED` — or worse, becomes a MEMBER rather than a method —
// this TU is where it shows up, and `check-cpp-capability-layout` is where the
// member case shows up as a number.
//
// Its sibling `ported_create_publisher_freestanding_probe.cpp` is the NEGATIVE
// half: the ported `create_publisher<M>("chatter", 10)` must FAIL here.

#include <nros/nros.hpp>

#include <cstddef>
#include <cstdint>

namespace nros_cpp_one_node_type_freestanding_test {

struct CounterMsg {
    int32_t data;
    static const size_t SERIALIZED_SIZE_MAX = 16;
    static constexpr const char* TYPE_NAME = "std_msgs::msg::dds_::Int32_";
    static constexpr const char* TYPE_HASH = "RIHS01_int32_stub";
    static int ffi_publish(void*, const void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
        if (out) *out = 0;
        return 0;
    }
    static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
};

/// The standalone freestanding shape from RFC-0089 §"Usage — standalone,
/// freestanding". Every line is `rclcpp::`; nothing names `nros::`.
inline int standalone_main() {
    // `rclcpp::init()` returns `void`, as upstream's does — a PORTED api keeps
    // upstream's channel even when checking it means asking a second verb.
    rclcpp::init();
    if (!rclcpp::ok()) return 1;

    rclcpp::Node node("talker"); // no allocation; upstream's shape
    if (!node.ok()) return 1;    // replaces upstream's throw

    rclcpp::Publisher<CounterMsg> pub; // inline storage
    if (!node.create_publisher(pub, "chatter").ok()) return 1;

    while (rclcpp::ok()) {
        CounterMsg msg;
        msg.data = 0;
        (void)pub.publish(msg);
        (void)rclcpp::spin_once(100);
    }
    return 0;
}

/// The component shape — the firmware recommendation. No allocator, no
/// derivation, no vtable, and the timer callback is a member bound through
/// `create_wall_timer`'s template overload (phase-427 W3, which retired the
/// free `nros::bind_timer`).
class Talker {
    rclcpp::Publisher<CounterMsg> pub_;
    rclcpp::Timer timer_;
    int count_;

  public:
    Talker() : count_(0) {}

    void on_tick() { ++count_; }

    rclcpp::Result configure(rclcpp::Node& node) {
        NROS_TRY(node.create_publisher(pub_, "chatter"));
        return node.create_wall_timer<Talker, &Talker::on_tick>(timer_, 1000, this);
    }
};

/// `rclcpp::Timer` reaches a freestanding target — it is `nros::Timer`, which
/// needs no `<memory>`. Only the nested `SharedPtr` aliases were ever
/// hosted-only, and the deleted `TimerBase` (phase-430 W7) was hosted-only in
/// its entirety.
inline void the_ros2_timer_spelling_is_not_hosted_only() {
    rclcpp::Timer t;
    (void)t.is_valid();
}

/// A node's logger still reaches the log macros with no `<string>` anywhere:
/// `rclcpp::Logger` is a name plus the opaque handle, and the conversion to
/// `nros_logger_t` is what kept every native call site compiling when
/// `get_logger()` took upstream's return type (phase-427 W5).
inline void logger_reaches_a_freestanding_target(rclcpp::Node& node) {
    NROS_LOG_INFO(node.get_logger(), "up");
}

inline void probe_entry_points() {
    (void)standalone_main();
    the_ros2_timer_spelling_is_not_hosted_only();
    Talker t;
    rclcpp::Node n("talker");
    (void)t.configure(n);
    logger_reaches_a_freestanding_target(n);
}

} // namespace nros_cpp_one_node_type_freestanding_test
