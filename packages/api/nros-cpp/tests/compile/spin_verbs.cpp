// Issue 0338 — `spin` must mean the same thing here as in rclcpp, the C API and
// Rust: block until shutdown. It used to mean the OPPOSITE on `Executor` — the
// bounded form was called `spin` and there was no way to say "spin forever", so
// `exec.spin()` (what a user ports from rclcpp) did not compile, and reaching
// for `spin(ms)` instead silently returned early.
//
// A compile-time probe rather than a runtime one: the defect was the SHAPE of
// the API (which arities exist and what they mean), so the assertion that
// catches a regression is "these calls type-check with these signatures".
// Running a forever-spin in a unit test would need a second thread to shut the
// executor down and would prove less.

#include "nros/executor.hpp"
#include <type_traits>

namespace {

// `spin()` exists and takes no required argument — the rclcpp shape. If someone
// re-adds a required duration parameter, this stops compiling.
static_assert(std::is_same<decltype(std::declval<nros::Executor&>().spin()), nros::Result>::value,
              "Executor::spin() must exist with no required argument (rclcpp shape)");

// It also accepts the optional poll interval.
static_assert(
    std::is_same<decltype(std::declval<nros::Executor&>().spin(10)), nros::Result>::value,
    "Executor::spin(poll_ms) must exist");

// The BOUNDED verb is `spin_for(duration_ms[, poll_ms])`.
static_assert(
    std::is_same<decltype(std::declval<nros::Executor&>().spin_for(100u)), nros::Result>::value,
    "Executor::spin_for(duration_ms) must exist");
static_assert(
    std::is_same<decltype(std::declval<nros::Executor&>().spin_for(100u, 5)), nros::Result>::value,
    "Executor::spin_for(duration_ms, poll_ms) must exist");

// `spin_once` is unchanged.
static_assert(
    std::is_same<decltype(std::declval<nros::Executor&>().spin_once()), nros::Result>::value,
    "Executor::spin_once() must exist");

} // namespace

int main() { return 0; }
