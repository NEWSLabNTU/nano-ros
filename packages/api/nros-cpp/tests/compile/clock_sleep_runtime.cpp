// phase-515 P5 — `rclcpp::Clock::sleep_until` / `sleep_for`, upstream's
// signatures, on a real executor over the stub backend.
//
// The envelope under test (RFC-0021): the wait DRIVES the executor, so the
// transport and the callbacks keep running while it blocks, and the deadline is
// on the clock itself, so a ROS-time clock wakes when SIMULATED time arrives.
//
//   1. before `rclcpp::init` there is no executor: `false`, no sleep;
//   2. a steady sleep lasts AT LEAST its duration (a lower bound -- an upper
//      bound alone passes a sleep that never waited) and a wall timer fires
//      during it;
//   3. a deadline already passed answers `true` at once;
//   4. a deadline on another clock type is `false` (upstream throws);
//   5. from inside a callback it is `false` (the executor is not reentrant);
//   6. a ROS-time clock frozen by the override wakes when a timer ADVANCES the
//      simulated time, long before the same span of wall time.

#include "rclcpp/rclcpp.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <cstdio>

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

constexpr int64_t MS = 1000000;

int g_ticks = 0;
int g_inner_calls = 0;
bool g_inner_result = true;
struct rcl_clock_t g_ros_clock;
int64_t g_sim_ns = 0;

} // namespace

int main() {
    rclcpp::Clock steady(RCL_STEADY_TIME);

    // rclcpp's signatures, taken as pointers so a drift fails to compile.
    bool (rclcpp::Clock::*until_ctx)(rclcpp::Time, rclcpp::Context) = &rclcpp::Clock::sleep_until;
    bool (rclcpp::Clock::*for_ctx)(rclcpp::Duration, rclcpp::Context) = &rclcpp::Clock::sleep_for;
    (void)until_ctx;
    (void)for_ctx;

    // 1 -- no executor yet.
    {
        const rclcpp::Time t0 = steady.now();
        check(!steady.sleep_for(rclcpp::Duration::from_nanoseconds(20 * MS)),
              "before init a sleep has no executor to drive: false");
        check((steady.now() - t0).nanoseconds() < 20 * MS, "...and does not block");
    }

    if (!rclcpp::init_with_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "p5").ok()) {
        std::fprintf(stderr, "clock_sleep_runtime: init on the stub backend failed\n");
        return 1;
    }
    rclcpp::Node node("p5_sleep");
    check(node.ok(), "a node must come up on the stub backend");

    // 2 -- a steady sleep, with a wall timer running through it.
    rclcpp::TimerBase::SharedPtr ticker =
        node.create_wall_timer(rclcpp::Duration::from_nanoseconds(5 * MS), [] { ++g_ticks; });
    check(static_cast<bool>(ticker), "a 5 ms wall timer");
    {
        g_ticks = 0;
        const rclcpp::Time t0 = steady.now();
        const bool ok = steady.sleep_for(rclcpp::Duration::from_nanoseconds(60 * MS));
        const int64_t slept = (steady.now() - t0).nanoseconds();
        check(ok, "sleep_for(60 ms) reaches its deadline");
        check(slept >= 60 * MS, "...after AT LEAST 60 ms");
        check(g_ticks >= 3, "...with the executor's callbacks running during the wait");
    }
    (void)ticker->cancel();

    // 3 -- already past.
    {
        const rclcpp::Time t0 = steady.now();
        check(steady.sleep_until(t0 - rclcpp::Duration::from_nanoseconds(1 * MS)),
              "a deadline already passed is reached at once");
        check((steady.now() - t0).nanoseconds() < 20 * MS, "...without waiting");
    }

    // 4 -- another clock type.
    check(!steady.sleep_until(rclcpp::Time(0, 0, RCL_SYSTEM_TIME)),
          "a deadline on another clock type is refused");

    // 5 -- from inside a callback.
    rclcpp::TimerBase::SharedPtr inner =
        node.create_wall_timer(rclcpp::Duration::from_nanoseconds(1 * MS), [] {
            if (g_inner_calls++ == 0) {
                rclcpp::Clock c(RCL_STEADY_TIME);
                g_inner_result = c.sleep_for(rclcpp::Duration::from_nanoseconds(5 * MS));
            }
        });
    for (int i = 0; i < 200 && g_inner_calls == 0; ++i) {
        (void)rclcpp::spin_once(2);
    }
    (void)inner->cancel();
    check(g_inner_calls > 0, "the reentrancy probe ran");
    check(!g_inner_result, "a sleep from inside a callback is refused");

    // 6 -- simulated time: frozen at t0, advanced 50 ms of ROS time per 2 ms
    // wall tick, so 500 ms of ROS time passes in ~20 ms of wall time.
    g_ros_clock = rclc_clock_get_zero_initialized();
    check(rcl_clock_init(RCL_ROS_TIME, &g_ros_clock, nullptr) == RCL_RET_OK, "ROS clock init");
    g_sim_ns = 1000 * 1000 * MS;
    check(rcl_set_ros_time_override(&g_ros_clock, g_sim_ns) == RCL_RET_OK, "freeze /clock");
    rclcpp::Clock ros(RCL_ROS_TIME);
    check(ros.ros_time_is_active(), "the override drives the ROS clock");
    rclcpp::TimerBase::SharedPtr sim =
        node.create_wall_timer(rclcpp::Duration::from_nanoseconds(2 * MS), [] {
            g_sim_ns += 50 * MS;
            (void)rcl_set_ros_time_override(&g_ros_clock, g_sim_ns);
        });
    {
        const rclcpp::Time wall0 = steady.now();
        const rclcpp::Time until = ros.now() + rclcpp::Duration::from_nanoseconds(500 * MS);
        check(ros.sleep_until(until), "a ROS-time sleep wakes when simulated time arrives");
        check(ros.now() >= until, "...on the simulated clock");
        check((steady.now() - wall0).nanoseconds() < 400 * MS,
              "...long before the same span of wall time");
    }
    (void)sim->cancel();
    (void)rcl_disable_ros_time_override(&g_ros_clock);

    if (g_failures != 0) {
        std::fprintf(stderr, "clock_sleep_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("clock_sleep_runtime: OK\n");
    return 0;
}
