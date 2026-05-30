// SPDX-License-Identifier: Apache-2.0
//
// topic_state_monitor synthetic port — Phase 209.G (first iteration).
//
// Mirrors the shape of Autoware's `topic_state_monitor` (multi-topic liveness
// watchdog + per-topic diagnostic publish). Vendoring the upstream source is
// a follow-up (needs an Autoware checkout); this is the in-tree synthetic that
// exercises the same compat surface.
//
// Known gap surfaced by this port (filed against 209.A):
// nros's `create_subscription` callback overload is SFINAE-restricted to
// `void(*)(const M&)` plain function pointers — it rejects capturing lambdas
// and `std::function`, which rclcpp accepts freely. A port today must hoist
// per-subscription state into globals (this file) and use stateless callbacks,
// OR poll via `try_recv_raw` in the spin loop. Closing that gap (carry a
// `std::function` via the FFI's user_data slot) is the next 209.A follow-up.

#include <chrono>
#include <cstddef>
#include <memory>
#include <string>
#include <thread>

#include <rclcpp/rclcpp.hpp>
#include <diagnostic_updater/diagnostic_updater.hpp>
#include "std_msgs/std_msgs.hpp"

namespace topic_state_monitor {

// Per-topic state hoisted into globals because the nros callback subscription
// only accepts plain fn pointers (no capture). Two slots — a / b.
struct TopicSlot {
    const char* name;
    std::chrono::steady_clock::time_point last_seen;
    int64_t last_value = 0;
    std::shared_ptr<rclcpp::Subscription<std_msgs::msg::Int32>> sub;
};

static TopicSlot g_slot_a = {"a", std::chrono::steady_clock::now(), 0, nullptr};
static TopicSlot g_slot_b = {"b", std::chrono::steady_clock::now(), 0, nullptr};

static void on_a(const std_msgs::msg::Int32& msg) {
    g_slot_a.last_seen  = std::chrono::steady_clock::now();
    g_slot_a.last_value = msg.data;
}
static void on_b(const std_msgs::msg::Int32& msg) {
    g_slot_b.last_seen  = std::chrono::steady_clock::now();
    g_slot_b.last_value = msg.data;
}

static void report(const TopicSlot& s, diagnostic_updater::DiagnosticStatusWrapper& w) {
    const auto now    = std::chrono::steady_clock::now();
    const auto age_ms = std::chrono::duration_cast<std::chrono::milliseconds>(
                            now - s.last_seen).count();
    w.add("topic", std::string(s.name));
    w.add("count", static_cast<int>(s.last_value));
    w.add("age_ms", static_cast<int>(age_ms));
    if (age_ms > 2000) {
        w.summary(diagnostic_updater::ERROR, "stale (>2 s)");
    } else if (age_ms > 500) {
        w.summary(diagnostic_updater::WARN, "approaching stale");
    } else {
        w.summary(diagnostic_updater::OK, "live");
    }
}

class Monitor : public rclcpp::Node {
public:
    Monitor() : rclcpp::Node("topic_state_monitor") {}

    void post_init() {
        g_slot_a.sub = this->create_subscription<std_msgs::msg::Int32>("a", 10, &on_a);
        g_slot_b.sub = this->create_subscription<std_msgs::msg::Int32>("b", 10, &on_b);

        updater_ = std::make_shared<diagnostic_updater::Updater>(
            shared_from_this(), /*period_seconds=*/1.0);
        updater_->setHardwareID("topic_state_monitor");
        updater_->add(g_slot_a.name,
                      [](diagnostic_updater::DiagnosticStatusWrapper& w) {
                          report(g_slot_a, w);
                      });
        updater_->add(g_slot_b.name,
                      [](diagnostic_updater::DiagnosticStatusWrapper& w) {
                          report(g_slot_b, w);
                      });
        RCLCPP_INFO(this->get_logger(), "topic_state_monitor up (2 topics)");
    }

    void tick() {
        if (updater_) {
            updater_->update();
        }
    }

private:
    std::shared_ptr<diagnostic_updater::Updater> updater_;
};

}  // namespace topic_state_monitor

int main(int argc, char** argv) {
    rclcpp::init(argc, argv);
    auto node = std::make_shared<topic_state_monitor::Monitor>();
    node->post_init();
    while (rclcpp::ok()) {
        node->tick();
        rclcpp::spin_some(node);
        std::this_thread::sleep_for(std::chrono::milliseconds(100));
    }
    rclcpp::shutdown();
    return 0;
}
