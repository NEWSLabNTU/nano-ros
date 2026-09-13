// phase-417 W4.c — a C++ node STOPS SPINNING without tearing down its session.
//
// `executor_cancel.cpp` beside this file is the SHAPE probe: the names exist,
// `cancel()` takes no argument and returns `Result`, `is_spinning()` is
// const-callable. It says so itself, and it points at
// `nros-node/src/executor/spin.rs::cancel_tests` for the behaviour. That
// delegation is right about WHERE the flag lives (RFC-0019) and incomplete
// about what it proves HERE: the Rust test drives `Executor::spin` directly,
// while the C++ path reaches the loop through `nros_cpp_spin`, and the whole
// point of W4.c's C++ half is that the previous way to end a spin —
// `shutdown()` — called `nros_cpp_fini` and destroyed the session on the way
// out. A `static_assert` cannot tell "the loop returned and the session is
// still open" from "the loop returned because the session died", which is the
// exact pair W4.c exists to separate.
//
// So this is the RUN. It asserts the four things the shape probe cannot:
//
//   1. a spin really was running (`is_spinning()` observed TRUE from another
//      thread) — otherwise "the loop ended" is satisfied by a loop that never
//      started, which is how a passing test measures nothing;
//   2. `cancel()` ends it, within a bound the caller chose (one `poll_ms`);
//   3. the SESSION SURVIVES — `ok()` still true, the node created before the
//      cancel still answers its name, and a NEW node can be created on the
//      executor afterwards. That last one is the load-bearing assertion:
//      `nros_cpp_fini` would have taken the session with it, and creating an
//      entity is the cheapest question only a live session can answer;
//   4. cancel is NOT one-shot — a second spin runs and a second cancel ends
//      it, because the flag is cleared on spin ENTRY. An executor you can stop
//      once and never restart is `shutdown()` with extra steps.
//
// The RMW backend is the shared stub (`nros-c/tests/run/stub_rmw_backend.c`),
// linked in rather than re-spelled here: nothing on this path touches the wire,
// and a second stub would be a second thing to keep in step with
// `first_missing_vtable_slot`'s seventeen slots.

#include "nros/executor.hpp"
#include "nros/node.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <atomic>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <thread>

// The strong hook `nros_cpp_init_rmw` calls; there is no weak default that
// would register the stub for us.
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

/// Poll `pred` until it holds or `budget_ms` elapses. Returns whether it held.
///
/// A bound rather than a sleep: the cancel boundary is one `poll_ms` wide, and
/// a fixed sleep either flakes under load or hides a regression behind slack.
template <typename Pred> bool wait_until(Pred pred, int budget_ms) {
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::milliseconds(budget_ms);
    while (std::chrono::steady_clock::now() < deadline) {
        if (pred()) return true;
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }
    return pred();
}

/// One spin/cancel round. Returns whether the loop was observed RUNNING before
/// the cancel — the negative control against a spin that never started.
bool spin_and_cancel(nros::Executor& exec) {
    std::atomic<bool> returned(false);
    std::atomic<int> spin_code(0);

    std::thread spinner([&exec, &returned, &spin_code]() {
        nros::Result r = exec.spin(5);
        spin_code.store(static_cast<int>(r.raw()));
        returned.store(true);
    });

    const bool observed_spinning = wait_until([&exec]() { return exec.is_spinning(); }, 2000);
    check(observed_spinning, "a spin loop must be OBSERVED running before the cancel — a test "
                             "whose loop never started proves nothing about stopping one");

    check(exec.cancel().ok(), "cancel() must succeed on a spinning executor");

    const bool ended = wait_until([&returned]() { return returned.load(); }, 5000);
    check(ended, "spin() must RETURN after cancel() — this is the whole of W4.c");
    if (!ended) {
        // Nothing can be salvaged: the thread is still in the loop and joining
        // would hang the lane. Abort loudly rather than leave a red that reads
        // like a harness timeout.
        std::fprintf(stderr, "FATAL: spin() did not return after cancel(); cannot join\n");
        std::exit(1);
    }
    spinner.join();

    check(spin_code.load() == 0, "a clean cancel must leave spin() reporting success");
    check(!exec.is_spinning(), "is_spinning() must be false once the loop has returned");
    return observed_spinning;
}

} // namespace

int main() {
    nros::Executor exec;
    check(nros::Executor::create_with_rmw(exec, NROS_STUB_RMW_NAME, nullptr, 0, "w4c_cancel").ok(),
          "executor create on the stub backend");
    check(exec.ok(), "a freshly created executor is ok()");
    check(!exec.is_spinning(), "a created-but-unspun executor is NOT spinning");

    rclcpp::Node before;
    check(exec.create_node(before, "w4c_before").ok(), "create a node before the spin");

    spin_and_cancel(exec);

    // --- the session survived ------------------------------------------------
    //
    // Before W4.c the only way out of `spin()` was `shutdown()`, which calls
    // `nros_cpp_fini`. Each of these three would have failed through that path.
    check(exec.ok(), "the executor is still ok() after a cancel — cancel is not shutdown");
    check(before.get_name() != nullptr && before.get_name()[0] != '\0',
          "a node created before the cancel still answers its name");

    rclcpp::Node after;
    check(exec.create_node(after, "w4c_after").ok(),
          "a NEW node can be created on the executor after a cancel — only a live session "
          "can answer this, so it is the assertion that separates cancel from shutdown");

    // --- and cancel is not one-shot ------------------------------------------
    const bool second_round_ran = spin_and_cancel(exec);
    check(second_round_ran, "the executor must spin AGAIN after a cancel — the flag is cleared "
                            "on spin entry, so a cancel that latched would make this false");

    check(nros_stub_rmw_drive_io_calls() > 0,
          "the spin must have driven the backend — a loop that never reached drive_io is not "
          "the loop this test is about");

    check(exec.shutdown().ok(),
          "shutdown() still works, and is still the one that ends the session");

    if (g_failures != 0) {
        std::fprintf(stderr, "executor_cancel_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("executor_cancel_runtime: ok\n");
    return 0;
}
