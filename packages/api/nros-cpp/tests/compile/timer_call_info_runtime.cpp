// issue 1974 — Jazzy's timer additions on the C++ surface:
//
//   * `rclcpp::TimerInfo` / `TimerInfoCallbackType` — a timer callback that
//     receives the activation's expected and actual call time;
//   * `autostart` — `create_wall_timer(period, cb, group, autostart)` and
//     `rclcpp::create_timer(node, clock, period, cb, group, autostart)` create
//     the timer CANCELLED when it is false, and `reset()` starts it.
//
// A RUN because each would compile as an inert stub: an info struct filled
// with zeros, an `autostart` nobody reads. The ROS-clock half is driven by the
// process-global ROS time override, so every time is one this test wrote and
// the assertions are equalities; the wall-timer half asserts the LOWER bound
// (`actual >= expected`), the property that holds on any clock.
//
// The RMW backend is the shared stub (`nros-c/tests/run/stub_rmw_backend.c`).

#include "rclcpp/rclcpp.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <cstdio>
#include <utility>

extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

namespace {

int g_failures = 0;

void check(bool cond, const char* what) {
    if (!cond) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s\n", what);
    }
}

int g_info_fires = 0;
rclcpp::TimerInfo g_last_info;
int g_late_fires = 0;
int g_wall_fires = 0;
rclcpp::TimerInfo g_wall_info;
int g_alias_fires = 0;

void spin_n(int n) {
    for (int i = 0; i < n; ++i) {
        (void)rclcpp::spin_once(1);
    }
}

constexpr int64_t MS = 1000000;

} // namespace

int main() {
    if (!rclcpp::init_with_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "i1974").ok()) {
        std::fprintf(stderr, "timer_call_info_runtime: init on the stub backend failed\n");
        return 1;
    }
    rclcpp::Node node("i1974_timers");
    check(node.ok(), "a node must come up on the stub backend");

    // The node clock is ROS time; freeze the simulated clock at t0.
    struct rcl_clock_t ros_clock = rclc_clock_get_zero_initialized();
    check(rcl_clock_init(RCL_ROS_TIME, &ros_clock, nullptr) == RCL_RET_OK, "ROS clock init");
    const int64_t t0 = 5000 * MS;
    check(rcl_set_ros_time_override(&ros_clock, t0) == RCL_RET_OK, "freeze /clock at t0");

    // --- TimerInfo on a ROS-clock timer --------------------------------------
    rclcpp::TimerBase::SharedPtr info_timer = rclcpp::create_timer(
        node, node.get_clock(), rclcpp::Duration::from_nanoseconds(10 * MS),
        [](const rclcpp::TimerInfo& info) {
            g_last_info = info;
            ++g_info_fires;
        });
    check(static_cast<bool>(info_timer), "create a TimerInfo-callback timer on the ROS clock");

    // --- autostart = false ---------------------------------------------------
    rclcpp::TimerBase::SharedPtr late_timer =
        rclcpp::create_timer(node, node.get_clock(), rclcpp::Duration::from_nanoseconds(10 * MS),
                             [] { ++g_late_fires; }, nullptr, false);
    check(static_cast<bool>(late_timer), "create an autostart=false timer");
    check(late_timer->is_canceled(), "autostart=false creates the timer CANCELLED");

    spin_n(20);
    check(g_info_fires == 0, "a ROS-clock timer waits for /clock");
    check(g_late_fires == 0, "a cancelled timer does not fire");

    // 35 ms of simulated time against a 10 ms period: due at t0 + 10 ms, run
    // at t0 + 35 ms.
    check(rcl_set_ros_time_override(&ros_clock, t0 + 35 * MS) == RCL_RET_OK, "/clock +35 ms");
    spin_n(3);
    check(g_info_fires == 1, "the TimerInfo timer fired once (Skip coalesces the backlog)");
    check(g_last_info.actual_call_time.nanoseconds() == t0 + 35 * MS,
          "actual_call_time is now on the timer's ROS clock");
    check(g_last_info.expected_call_time.nanoseconds() == t0 + 10 * MS,
          "expected_call_time is the deadline the activation served");
    check(g_last_info.actual_call_time.get_clock_type() == RCL_ROS_TIME,
          "TimerInfo times carry the timer's clock type");
    check(g_last_info.actual_call_time.nanoseconds() >=
              g_last_info.expected_call_time.nanoseconds(),
          "an activation never runs before its deadline");
    check(g_late_fires == 0, "still cancelled after /clock moved");

    check(late_timer->reset().ok(), "reset() starts the autostart=false timer");
    check(!late_timer->is_canceled(), "reset() un-cancels");
    spin_n(3);
    check(g_late_fires == 0, "the first period counts from the reset");
    check(rcl_set_ros_time_override(&ros_clock, t0 + 45 * MS) == RCL_RET_OK, "/clock +10 ms");
    spin_n(3);
    check(g_late_fires == 1, "fires one period after the reset");

    info_timer.reset();
    late_timer.reset();

    // --- TimerInfo on a WALL timer, plus the alias type ----------------------
    rclcpp::TimerBase::SharedPtr wall_timer =
        node.create_wall_timer(rclcpp::Duration::from_nanoseconds(1 * MS),
                               [](const rclcpp::TimerInfo& info) {
                                   g_wall_info = info;
                                   ++g_wall_fires;
                               });
    check(static_cast<bool>(wall_timer), "create a TimerInfo-callback wall timer");
    rclcpp::TimerInfoCallbackType alias_cb = [](const rclcpp::TimerInfo& info) {
        if (info.actual_call_time.nanoseconds() >= info.expected_call_time.nanoseconds()) {
            ++g_alias_fires;
        }
    };
    rclcpp::TimerBase::SharedPtr alias_timer = node.create_wall_timer(
        rclcpp::Duration::from_nanoseconds(1 * MS), std::move(alias_cb), nullptr, true);
    check(static_cast<bool>(alias_timer), "a TimerInfoCallbackType variable is a timer callback");
    for (int i = 0; i < 200 && (g_wall_fires == 0 || g_alias_fires == 0); ++i) {
        (void)rclcpp::spin_once(2);
    }
    check(g_wall_fires > 0, "the wall timer fired");
    check(g_alias_fires > 0, "the TimerInfoCallbackType timer fired with actual >= expected");
    check(g_wall_info.actual_call_time.get_clock_type() == RCL_STEADY_TIME,
          "a wall timer's call times are on the steady clock");
    check(g_wall_info.actual_call_time.nanoseconds() >= g_wall_info.expected_call_time.nanoseconds(),
          "lower bound: a wall activation never runs before its deadline");

    if (g_failures != 0) {
        std::fprintf(stderr, "timer_call_info_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("timer_call_info_runtime: ok (wall=%d alias=%d)\n", g_wall_fires, g_alias_fires);
    return 0;
}
