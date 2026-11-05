// Phase 536 W2 (RFC-0052 "Names") -- a scheduling tier is a sched task. The
// C++ board entry's `run_tiers` / `NativeTierSpec` are `run_sched_tasks` /
// `NativeSchedTaskSpec`; the old names stay one release as deprecated
// aliases. This TU pins both spellings (the lane greps the deprecation
// warnings) and that both resolve to the same thing. Compile-only.

#include "rclcpp/detail/main.hpp"

#include <type_traits>

namespace {

int32_t new_spelling(const rclcpp::board::NativeSchedTaskSpec* t, size_t n) {
    return rclcpp::board::LinuxBoard::run_sched_tasks("s", "/ns", t, n);
}

#if defined(__GNUC__)
#pragma GCC diagnostic push
#pragma GCC diagnostic warning "-Wdeprecated-declarations"
#endif
static_assert(std::is_same<rclcpp::board::NativeTierSpec, rclcpp::board::NativeSchedTaskSpec>::value,
              "NativeTierSpec is an alias of NativeSchedTaskSpec");

int32_t old_spelling(const rclcpp::board::NativeTierSpec* t, size_t n) {
    return rclcpp::board::LinuxBoard::run_tiers("s", "/ns", t, n);
}
#if defined(__GNUC__)
#pragma GCC diagnostic pop
#endif

} // namespace

int32_t (*const kNew)(const rclcpp::board::NativeSchedTaskSpec*, size_t) = new_spelling;
int32_t (*const kOld)(const rclcpp::board::NativeSchedTaskSpec*, size_t) = old_spelling;
