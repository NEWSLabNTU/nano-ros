/// @file main.cpp
/// @brief Two `rclcpp_lifecycle::LifecycleNode`s in one image, each with its
/// own lifecycle.
///
/// `/lc_alpha` and `/lc_beta` are two managed nodes. Each registers its own
/// five REP-2002 services and state machine, so `ros2 lifecycle` drives one
/// without moving the other:
///
/// ```console
/// $ ./cpp_lifecycle_pair &
/// $ ros2 lifecycle nodes
/// $ ros2 lifecycle set /lc_alpha configure
/// $ ros2 lifecycle set /lc_alpha activate
/// $ ros2 topic echo /lc_alpha/chatter std_msgs/msg/String
/// ```
///
/// Each node publishes `<node>: <n>` on `/<node>/chatter` through a
/// `LifecyclePublisher` from one shared timer. Nothing goes out until that
/// node is `active`; publishing stops at `deactivate` and resumes at the next
/// `activate`. Each `on_activate` also publishes one `<node>: activated`
/// line, which goes out because a node's publishers are active by the time its
/// `on_activate` runs, as in rclcpp.
///
/// Every transition callback prints `<node> on_<transition>`, so a test can
/// see which node's own code ran. `NROS_SPIN_MS` bounds the run; unset, it
/// spins until killed.

#include <stdio.h>
#include <stdlib.h>

#include <rclcpp/rclcpp.hpp>
#include <rclcpp_lifecycle/lifecycle_node.hpp>
#include <rclcpp_lifecycle/lifecycle_publisher.hpp>

#include "std_msgs.hpp"

class PairTalker : public rclcpp_lifecycle::LifecycleNode {
  public:
    explicit PairTalker(const char* name) : rclcpp_lifecycle::LifecycleNode(name, "/") {
        snprintf(topic_, sizeof topic_, "/%s/chatter", name);
    }

    /// One tick of the shared timer: publish the next count. A node that is
    /// not active drops it.
    void tick() {
        char text[16];
        snprintf(text, sizeof text, "%d", ++count_);
        say(text);
    }

    CallbackReturn on_configure(const rclcpp_lifecycle::State&) override {
        pub_ = create_publisher<std_msgs::msg::String>(topic_, 10);
        printf("%s on_configure\n", get_name());
        return pub_.get() != nullptr ? CallbackReturn::SUCCESS : CallbackReturn::FAILURE;
    }

    CallbackReturn on_activate(const rclcpp_lifecycle::State& state) override {
        LifecycleNode::on_activate(state);
        printf("%s on_activate\n", get_name());
        say("activated");
        return CallbackReturn::SUCCESS;
    }

    CallbackReturn on_deactivate(const rclcpp_lifecycle::State& state) override {
        LifecycleNode::on_deactivate(state);
        printf("%s on_deactivate\n", get_name());
        return CallbackReturn::SUCCESS;
    }

    CallbackReturn on_cleanup(const rclcpp_lifecycle::State&) override {
        pub_.reset();
        printf("%s on_cleanup\n", get_name());
        return CallbackReturn::SUCCESS;
    }

    CallbackReturn on_shutdown(const rclcpp_lifecycle::State&) override {
        printf("%s on_shutdown\n", get_name());
        return CallbackReturn::SUCCESS;
    }

  private:
    void say(const char* text) {
        if (pub_.get() == nullptr) {
            return;
        }
        char payload[48];
        snprintf(payload, sizeof payload, "%s: %s", get_name(), text);
        std_msgs::msg::String msg;
        msg.data = payload;
        (void)pub_->publish(msg);
    }

    rclcpp_lifecycle::LifecyclePublisher<std_msgs::msg::String>::SharedPtr pub_;
    char topic_[40] = {};
    int count_ = 0;
};

static PairTalker* g_nodes[2] = {nullptr, nullptr};

static void timer_callback(void*) {
    for (PairTalker* n : g_nodes) {
        n->tick();
    }
}

/* How long to spin, in ms. Unset or 0: until the process is killed. */
static unsigned spin_ms() {
    const char* s = getenv("NROS_SPIN_MS");
    if (s == nullptr || *s == '\0') {
        return 0u;
    }
    long v = strtol(s, nullptr, 10);
    return v > 0 ? static_cast<unsigned>(v) : 0u;
}

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;
    // Line-buffer stdout: glibc full-buffers non-tty stdout, so when piped to
    // a test harness each line must flush on its newline.
#ifdef _IOLBF
    setvbuf(stdout, nullptr, _IOLBF, 0);
#endif
    if (!rclcpp::init_in().ok()) {
        fprintf(stderr, "rclcpp::init_in failed\n");
        return 1;
    }

    PairTalker alpha("lc_alpha");
    PairTalker beta("lc_beta");
    g_nodes[0] = &alpha;
    g_nodes[1] = &beta;
    for (PairTalker* n : g_nodes) {
        printf("Lifecycle services registered on /%s (state %d)\n", n->get_name(),
               static_cast<int>(n->get_current_state().id()));
    }

    // One timer for both nodes: each tick tries both publishers.
    rclcpp::Timer timer;
    if (!alpha.create_wall_timer(timer, 200, timer_callback, nullptr).ok()) {
        fprintf(stderr, "create_wall_timer failed\n");
        return 2;
    }

    printf("Ready. Drive them with `ros2 lifecycle set /lc_alpha configure`.\n");

    const unsigned budget = spin_ms();
    for (unsigned waited = 0; budget == 0u || waited < budget; waited += 100) {
        (void)rclcpp::spin_once(100);
    }
    rclcpp::shutdown();
    return 0;
}
