// Issue 2144 (RFC-0112 D2.2, "Readiness out") — a C++ host learns that work
// arrived through `rclcpp::Executor::set_ready_notifier`, instead of polling.
//
// The C probe (`nros-c/tests/run/ready_notifier.c`) checks the same contract on
// the C path; this one checks that the C++ wrapper and its FFI
// (`nros_cpp_executor_set_ready_notifier`) reach the executor. Arrivals come
// from the stub backend's own wake path (`nros_stub_rmw_invoke_wake`), which is
// what a real backend's worker thread does on a datagram.

#include "rclcpp/executor.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <atomic>
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

std::atomic<int> g_calls[2];

void on_ready(void* ctx) {
    g_calls[reinterpret_cast<std::intptr_t>(ctx)].fetch_add(1);
}

} // namespace

int main() {
    rclcpp::Executor exec;
    check(!exec.set_ready_notifier(on_ready, nullptr).ok(),
          "an executor that was never created refuses");
    check(rclcpp::Executor::create_with_rmw(exec, NROS_STUB_RMW_NAME, nullptr, 0, "ready_notifier")
              .ok(),
          "executor create on the stub backend");

    // Negative control: nothing installed, an arrival calls nothing.
    check(nros_stub_rmw_invoke_wake(), "the backend holds the executor's wake callback");
    check(g_calls[0].load() + g_calls[1].load() == 0, "an arrival before the install notified");

    check(exec.set_ready_notifier(on_ready, reinterpret_cast<void*>(std::intptr_t{0})).ok(),
          "the stub wakes asynchronously, so the install succeeds");
    check(nros_stub_rmw_invoke_wake(), "arrival");
    check(g_calls[0].load() == 1, "an arrival with nothing spinning did not notify");

    check(exec.set_ready_notifier(on_ready, reinterpret_cast<void*>(std::intptr_t{1})).ok(),
          "replaced");
    check(nros_stub_rmw_invoke_wake(), "arrival");
    check(g_calls[0].load() == 1 && g_calls[1].load() == 1,
          "a replaced notifier's context was called again");

    check(exec.set_ready_notifier(nullptr, nullptr).ok(), "cleared");
    check(nros_stub_rmw_invoke_wake(), "arrival");
    check(g_calls[0].load() == 1 && g_calls[1].load() == 1, "a cleared notifier was called");
    check(exec.spin_some().ok(), "the spin a notification prompts still runs");

    if (g_failures != 0) {
        std::fprintf(stderr, "ready_notifier_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("ready_notifier_runtime: OK\n");
    return 0;
}
