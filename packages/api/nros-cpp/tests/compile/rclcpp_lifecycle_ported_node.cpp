// phase-482 W4 — a ported lifecycle node's CLASS BODY compiles unchanged.
//
// The shape is upstream's `lifecycle_talker` demo (ros2/demos, `lifecycle`),
// with two edits RFC-0096 D5 already lists and nothing else:
//   * the members are spelled `X::SharedPtr`, not `std::shared_ptr<X>` (D5 item 1);
//   * the message is `std_msgs/msg/Int32` built in place, because this probe is
//     about the lifecycle surface, not about string messages.
// Everything the demo writes against `rclcpp_lifecycle::` is verbatim: the base
// constructor taking a name, the six `on_*` overrides with upstream's
// signatures and the fully qualified return type, `LifecycleNode::on_activate`
// called from the override, `create_publisher<M>(topic, depth)` returning a
// managed publisher, `is_activated()`, `publish`, `get_current_state().label()`,
// and resetting the handles in `on_cleanup`.
//
// `just check cpp` compiles it with `-DNROS_CPP_STD=1` (the ported flavour) AND
// without it, so the class body is freestanding too.

#include <rclcpp/rclcpp.hpp>
#include <rclcpp_lifecycle/lifecycle_node.hpp>
#include <rclcpp_lifecycle/lifecycle_publisher.hpp>

namespace std_msgs {
namespace msg {
// Mirror of a codegen'd message (cf. std_msgs/msg/Int32).
struct Int32 {
    int32_t data{0};
    static constexpr const char* TYPE_NAME = "std_msgs::msg::dds_::Int32_";
    static constexpr const char* TYPE_HASH = "RIHS01_stub";
    static constexpr ::size_t SERIALIZED_SIZE_MAX = 8;
    static int ffi_publish(void*, const void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t*, ::size_t, ::size_t* len) {
        *len = 0;
        return 0;
    }
    static int ffi_deserialize(const uint8_t*, ::size_t, void*) { return 0; }
};
} // namespace msg
} // namespace std_msgs

class LifecycleTalker : public rclcpp_lifecycle::LifecycleNode {
  public:
    explicit LifecycleTalker(const char* node_name) : rclcpp_lifecycle::LifecycleNode(node_name) {}

    void publish() {
        std_msgs::msg::Int32 msg;
        msg.data = ++count_;
        if (!pub_->is_activated()) {
            RCLCPP_INFO(get_logger(), "Lifecycle publisher is currently inactive.");
        } else {
            RCLCPP_INFO(get_logger(), "Lifecycle publisher is active. Publishing: [%d]",
                        static_cast<int>(msg.data));
        }
        (void)pub_->publish(msg);
    }

    rclcpp_lifecycle::node_interfaces::LifecycleNodeInterface::CallbackReturn
    on_configure(const rclcpp_lifecycle::State&) override {
        pub_ = this->create_publisher<std_msgs::msg::Int32>("lifecycle_chatter", 10);
        RCLCPP_INFO(get_logger(), "on_configure() is called.");
        return rclcpp_lifecycle::node_interfaces::LifecycleNodeInterface::CallbackReturn::SUCCESS;
    }

    rclcpp_lifecycle::node_interfaces::LifecycleNodeInterface::CallbackReturn
    on_activate(const rclcpp_lifecycle::State& state) override {
        LifecycleNode::on_activate(state);
        RCLCPP_INFO(get_logger(), "on_activate() is called.");
        return rclcpp_lifecycle::node_interfaces::LifecycleNodeInterface::CallbackReturn::SUCCESS;
    }

    rclcpp_lifecycle::node_interfaces::LifecycleNodeInterface::CallbackReturn
    on_deactivate(const rclcpp_lifecycle::State& state) override {
        LifecycleNode::on_deactivate(state);
        RCLCPP_INFO(get_logger(), "on_deactivate() is called.");
        return rclcpp_lifecycle::node_interfaces::LifecycleNodeInterface::CallbackReturn::SUCCESS;
    }

    rclcpp_lifecycle::node_interfaces::LifecycleNodeInterface::CallbackReturn
    on_cleanup(const rclcpp_lifecycle::State&) override {
        pub_.reset();
        RCLCPP_INFO(get_logger(), "on cleanup is called.");
        return rclcpp_lifecycle::node_interfaces::LifecycleNodeInterface::CallbackReturn::SUCCESS;
    }

    rclcpp_lifecycle::node_interfaces::LifecycleNodeInterface::CallbackReturn
    on_shutdown(const rclcpp_lifecycle::State& state) override {
        pub_.reset();
        RCLCPP_INFO(get_logger(), "on shutdown is called from state %s.", state.label());
        return rclcpp_lifecycle::node_interfaces::LifecycleNodeInterface::CallbackReturn::SUCCESS;
    }

  private:
    rclcpp_lifecycle::LifecyclePublisher<std_msgs::msg::Int32>::SharedPtr pub_;
    int32_t count_ = 0;
};

// The transitions a `main` (or `ros2 lifecycle set`) drives, and the state each
// leaves the node in.
inline const char* drive(LifecycleTalker& node) {
    (void)node.configure();
    (void)node.activate();
    node.publish();
    (void)node.deactivate();
    const rclcpp_lifecycle::State& s = node.cleanup();
    return s.label();
}

// Callbacks WITHOUT subclassing, and the transition graph, on the same node.
inline rclcpp_lifecycle::LifecycleNode::CallbackReturn
on_configure_cb(const rclcpp_lifecycle::State&, void*) {
    return rclcpp_lifecycle::LifecycleNode::CallbackReturn::SUCCESS;
}
inline bool count_transition(void* ctx, const rclcpp_lifecycle::Transition&) {
    ++*static_cast<int*>(ctx);
    return true;
}
inline int extras(rclcpp_lifecycle::LifecycleNode& node) {
    node.register_on_configure(&on_configure_cb, nullptr);
    int rows = 0;
    (void)node.get_transition_graph(&count_transition, &rows);
    return rows;
}
