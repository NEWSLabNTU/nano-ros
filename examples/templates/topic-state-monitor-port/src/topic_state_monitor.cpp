// SPDX-License-Identifier: Apache-2.0
//
// topic_state_monitor synthetic port — Phase 209.G (post 209.A.follow-up).
//
// Mirrors the shape of Autoware's `topic_state_monitor` (multi-topic liveness
// watchdog + per-topic diagnostic publish). After the 209.A.follow-up landed,
// **capturing-lambda subscription callbacks compile unchanged** — this file
// uses the natural rclcpp `[this, &state](const M&) { ... }` shape no
// upstream port would have to rewrite.
//
// Vendoring the real upstream source is still a follow-up (needs an Autoware
// checkout + the `autoware_*` msg codegen path); this is the in-tree
// synthetic that exercises the same compat surface.

#include <chrono>
#include <memory>
#include <string>
#include <thread>
#include <vector>

#include <rclcpp/rclcpp.hpp>
#include <diagnostic_updater/diagnostic_updater.hpp>
#include "std_msgs/std_msgs.hpp"

namespace topic_state_monitor {

struct TopicState {
    std::string name;
    std::chrono::steady_clock::time_point last_seen;
    int64_t last_value = 0;
    std::shared_ptr<rclcpp::Subscription<std_msgs::msg::Int32>> sub;
};

class Monitor : public rclcpp::Node {
public:
    Monitor() : rclcpp::Node("topic_state_monitor"), topics_(2) {
        topics_[0].name = "a";
        topics_[1].name = "b";
        const auto now = std::chrono::steady_clock::now();
        for (auto& t : topics_) {
            t.last_seen = now;
        }
    }

    void post_init() {
        // Capturing-lambda subscriptions — the rclcpp idiom every port writes.
        // The compat header's pump-based dispatch makes this compile (and
        // dispatch through `rclcpp::spin_some` / `spin`).
        for (auto& t : topics_) {
            TopicState* state = &t;
            t.sub = this->create_subscription<std_msgs::msg::Int32>(
                t.name, 10,
                [state](const std_msgs::msg::Int32& msg) {
                    state->last_seen  = std::chrono::steady_clock::now();
                    state->last_value = msg.data;
                });
        }

        updater_ = std::make_shared<diagnostic_updater::Updater>(
            shared_from_this(), /*period_seconds=*/1.0);
        updater_->setHardwareID("topic_state_monitor");
        for (auto& t : topics_) {
            TopicState* state = &t;
            updater_->add(t.name,
                          [this, state](diagnostic_updater::DiagnosticStatusWrapper& w) {
                              report_topic(*state, w);
                          });
        }
        RCLCPP_INFO(this->get_logger(), "topic_state_monitor up (%zu topics)",
                    static_cast<size_t>(topics_.size()));
    }

    void tick() {
        if (updater_) {
            updater_->update();
        }
    }

private:
    void report_topic(const TopicState& t,
                      diagnostic_updater::DiagnosticStatusWrapper& w) {
        const auto now    = std::chrono::steady_clock::now();
        const auto age_ms = std::chrono::duration_cast<std::chrono::milliseconds>(
                                now - t.last_seen).count();
        w.add("topic", t.name);
        w.add("count", static_cast<int>(t.last_value));
        w.add("age_ms", static_cast<int>(age_ms));
        if (age_ms > 2000) {
            w.summary(diagnostic_updater::ERROR, "stale (>2 s)");
        } else if (age_ms > 500) {
            w.summary(diagnostic_updater::WARN, "approaching stale");
        } else {
            w.summary(diagnostic_updater::OK, "live");
        }
    }

    std::vector<TopicState> topics_;
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
