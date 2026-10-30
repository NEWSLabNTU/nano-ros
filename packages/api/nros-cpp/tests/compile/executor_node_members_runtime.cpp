// phase-515 P1/P2 — rclcpp's executor members, called with upstream's
// signatures on a real `rclcpp::Executor` over the shared stub RMW backend:
//
//   * `spin_until_future_complete(future[, timeout])` drives THIS executor
//     (not the process one — no `rclcpp::init` here) until the future is
//     ready, returns SUCCESS without burning the timeout, and returns TIMEOUT
//     no EARLIER than the budget for a future that never completes;
//   * `add_node` on a node this executor created is a no-op (a foreign node
//     aborts, which a single-process test cannot observe without dying);
//   * `spin_node_some` / `spin_node_once` spin through that check;
//   * `spin_all(0)` is refused, as rclcpp throws for it; `spin_all(n)` drains;
//   * issue 2143: `next_deadline_us` -- the time-out a host loop sleeps on --
//     is "no deadline" with nothing scheduled, and a 10 ms wall timer is due
//     within 10 ms and not already.
//
// `remove_node` is a compile-time refusal, held by
// `ros2_refuse_remove_node_probe.cpp`.

#include "rclcpp/rclcpp.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <chrono>
#include <cstdint>
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

/// A future that becomes ready on its `n`-th poll, counting the polls, so a
/// SUCCESS is proven to have come from polling between spins.
struct CountdownFuture {
    int remaining;
    int polls;
    bool is_ready() {
        ++polls;
        return --remaining <= 0;
    }
};

struct NeverFuture {
    bool is_ready() { return false; }
};

} // namespace

int main() {
    rclcpp::Executor exec;
    check(rclcpp::Executor::create_with_rmw(exec, NROS_STUB_RMW_NAME, nullptr, 0, "p515_exec").ok(),
          "create an explicit executor on the stub backend");
    rclcpp::Node node;
    check(exec.create_node(node, "p515_node").ok(), "create a node on it");
    rclcpp::Node::SharedPtr handle = node.shared_from_this();

    // P1 — completes after a few polls: SUCCESS, well inside a long budget.
    CountdownFuture soon{3, 0};
    const auto t0 = std::chrono::steady_clock::now();
    check(exec.spin_until_future_complete(soon, 5000) == rclcpp::FutureReturnCode::SUCCESS,
          "a future that completes returns SUCCESS");
    const auto took = std::chrono::duration_cast<std::chrono::milliseconds>(
        std::chrono::steady_clock::now() - t0);
    check(soon.polls >= 3, "the future was polled until it completed");
    check(took.count() < 1000, "SUCCESS does not burn the timeout");

    // P1 — never completes: TIMEOUT, and not before the budget.
    NeverFuture never;
    const auto t1 = std::chrono::steady_clock::now();
    check(exec.spin_until_future_complete(never, 50) == rclcpp::FutureReturnCode::TIMEOUT,
          "a future that never completes times out");
    const auto waited = std::chrono::duration_cast<std::chrono::milliseconds>(
        std::chrono::steady_clock::now() - t1);
    check(waited.count() >= 50, "TIMEOUT is not reported before the budget");

    // P2 — the node-scoped verbs on this executor's own node.
    exec.add_node(handle);
    exec.add_node(handle, false);
    check(exec.spin_node_some(handle).ok(), "spin_node_some on an own node");
    check(exec.spin_node_once(handle, 0).ok(), "spin_node_once on an own node");

    // P2 — spin_all: zero refused (upstream throws), a bound drains.
    check(exec.spin_all(0).raw() == static_cast<int32_t>(rclcpp::ErrorCode::InvalidArgument),
          "spin_all(0) is refused");
    check(exec.spin_all(20).ok(), "spin_all(20) drains");

    // issue 2143 -- the host-loop time-out. The stub backend names no internal
    // deadline, so with no timer there is nothing to wake for.
    uint64_t next_us = 0;
    check(exec.next_deadline_us(next_us).ok(), "next_deadline_us is readable");
    check(next_us == UINT64_MAX, "nothing scheduled reads as no deadline (UINT64_MAX)");
    int timer_fires = 0;
    rclcpp::TimerHandle timer = node.create_wall_timer(
        rclcpp::Duration::from_nanoseconds(10 * 1000 * 1000), [&timer_fires] { ++timer_fires; });
    check(static_cast<bool>(timer), "a 10 ms wall timer");
    check(exec.next_deadline_us(next_us).ok(), "next_deadline_us is readable with a timer");
    check(next_us <= 10000, "a 10 ms wall timer is due within 10 ms");
    // The lower bound: an answer of 0 would turn the host's sleep into a busy
    // poll. Nothing has spun since the timer was registered.
    check(next_us >= 1000, "and is not already due");
    if (next_us > 10000 || next_us < 1000) {
        std::fprintf(stderr, "       next_deadline_us = %llu\n",
                     static_cast<unsigned long long>(next_us));
    }

    if (g_failures != 0) {
        std::fprintf(stderr, "executor_node_members_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("executor_node_members_runtime: OK\n");
    return 0;
}
