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
static_assert(std::is_same<decltype(std::declval<nros::Executor&>().spin(10)), nros::Result>::value,
              "Executor::spin(poll_ms) must exist");

// The BOUNDED verb is `spin_for(duration_ms[, poll_ms])`.
static_assert(
    std::is_same<decltype(std::declval<nros::Executor&>().spin_for(100u)), nros::Result>::value,
    "Executor::spin_for(duration_ms) must exist");
static_assert(
    std::is_same<decltype(std::declval<nros::Executor&>().spin_for(100u, 5)), nros::Result>::value,
    "Executor::spin_for(duration_ms, poll_ms) must exist");

// `spin_once` takes a REQUIRED budget — phase-417 stage 3. Upstream's default
// is -1 (block indefinitely) and ours cannot be, so there is no default to give
// that would not be a budget the caller never chose; the no-argument form is
// REFUSE-LOUD and `ros2_refuse_unbounded_spin_probe.cpp` is what proves it
// fires. This is the POSITIVE half — the budgeted form still type-checks and
// still returns `nros::Result`.
static_assert(
    std::is_same<decltype(std::declval<nros::Executor&>().spin_once(10)), nros::Result>::value,
    "Executor::spin_once(timeout_ms) must exist");

// Deliberately NOT asserted with `decltype(…spin_once())`: the refusal is a
// `static_assert` in a member-template BODY, and `decltype` does not instantiate
// a body. Such an assertion passes whether or not the refusal exists, which is
// the shape of a check that cannot fail. The expected-failure probe is the only
// thing that can answer it.

} // namespace

int main() {
    return 0;
}
