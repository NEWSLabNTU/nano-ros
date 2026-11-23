// A stub board's wall clock (RFC-0141 D2, phase-582 W1d), as a C++ component.
//
// Only the `linux_talker_clock_board` image launches this component, and that image states
// `[knobs.clock] source = "board"`, so the port asks the hook below on its first wall-clock read.
// The hook sits in this TU, so no other image links it.

#include "clock_board_pkg/ClockBoard.hpp"

#include <cstdint>
#include <cstdio>

#include <rclcpp/rclcpp.hpp>

namespace {
/// The instant the stub board answers: 2023-11-14T22:13:20Z.
constexpr std::uint64_t kBoardInstantNs = 1700000000000000000ULL;
} // namespace

/// The board's wall-clock hook: writes the board's instant and reports it set.
extern "C" bool nros_board_wall_clock_ns(std::uint64_t* out_ns) {
    if (out_ns == nullptr) {
        return false;
    }
    *out_ns = kBoardInstantNs;
    return true;
}

namespace clock_board_pkg {

void ClockBoard::on_tick() {
    const ::rclcpp::Clock clock(RCL_SYSTEM_TIME);
    const std::int64_t wall_ns = clock.now().nanoseconds();
    const nros_wall_clock_state_t st = ::rclcpp::Clock::wall_clock_state();
    // Casts, not <cinttypes>' PRId64/PRIu32: the workspace compiles this TU for
    // every board's image, and an embedded C++ library may not define those
    // macros at all.
    std::printf("clock-board: wall_ns=%lld synced=%d source=%u steps=%lu\n",
                static_cast<long long>(wall_ns), st.synced ? 1 : 0,
                static_cast<unsigned>(st.source), static_cast<unsigned long>(st.steps));
}

::rclcpp::Result ClockBoard::configure(::rclcpp::Node& node) {
    ::setvbuf(stdout, nullptr, _IONBF, 0);
    return node.create_wall_timer<ClockBoard, &ClockBoard::on_tick>(timer_, 1000, this);
}

} // namespace clock_board_pkg
