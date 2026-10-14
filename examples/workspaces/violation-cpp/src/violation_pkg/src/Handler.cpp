#include "violation_pkg/Handler.hpp"

#include <chrono>
#include <cstdio>

#if defined(__ZEPHYR__)
#include <zephyr/kernel.h>
#endif

namespace violation_pkg {

namespace {
// Busy-wait, not sleep: the overrun is CPU work inside the callback, which is
// what `max-latency-runtime` measures. On Zephyr it is `k_busy_wait`: on
// native_sim simulated time does not move while a thread spins on a clock,
// so a steady_clock loop never ends there; `k_busy_wait` advances it.
void busy_ms(int ms) {
#if defined(__ZEPHYR__)
    k_busy_wait(static_cast<uint32_t>(ms) * 1000u);
#else
    auto until = std::chrono::steady_clock::now() + std::chrono::milliseconds(ms);
    while (std::chrono::steady_clock::now() < until) {
    }
#endif
}
constexpr int RUN_AT_TICK = 3;
constexpr int OVERRUN_AT_TICK = 6;
// phase10-B3: the take of the sample this tick publishes on /t4/avail does
// 250 ms of work before returning, so the NEXT tick is late and the route
// from that take to /t4/answer exceeds its 180 ms while the tick itself is
// short.
constexpr int LATE_TICK_AFTER = 9;
} // namespace

void Handler::on_avail(const std_msgs::msg::Int32& msg) {
    if (msg.data == LATE_TICK_AFTER) {
        std::printf("[handler] took avail %d: 250 ms before the next tick\n", msg.data);
        busy_ms(250);
    }
}

void Handler::on_timer() {
    tick_++;
    std_msgs::msg::Int32 msg;
    msg.data = tick_;
    (void)pub_.publish(msg);
    // phase10-B3: the route's output (the answer to the last take of
    // /t4/avail), then this tick's sample of /t4/avail, which the handler
    // takes itself (same session, local delivery).
    (void)answer_.publish(msg);
    (void)avail_.publish(msg);
    if (tick_ == 1) {
        busy_ms(80); // INIT: start-up work over the budget, before arming
    }
    if (tick_ == RUN_AT_TICK) {
        std::printf("[handler] RUN at tick %d: arming the monitors\n", tick_);
        ::rclcpp::arm_monitors();
    }
    if (tick_ == OVERRUN_AT_TICK) {
        std::printf("[handler] overrun at tick %d: 150 ms\n", tick_);
        busy_ms(150);
    }
    std::printf("[handler] tick=%d\n", tick_);
}

Handler::Handler(::rclcpp::NodeHandle h) : ::rclcpp::NodeWithTimers<1>(h, "handler") {
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    pub_ = create_publisher_in<std_msgs::msg::Int32>("/t4/state");
    answer_ = create_publisher_in<std_msgs::msg::Int32>("/t4/answer");
    avail_ = create_publisher_in<std_msgs::msg::Int32>("/t4/avail");
    create_subscription_in<std_msgs::msg::Int32, Handler, &Handler::on_avail>(
        "/t4/avail", ::rclcpp::QoS(1));
    create_wall_timer_in<Handler, &Handler::on_timer>(100);
}

} // namespace violation_pkg
