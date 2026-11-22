#pragma once

#include <rclcpp/rclcpp.hpp>

namespace clock_board_pkg {

/// A stub board's wall clock (RFC-0141 D2, phase-582 W1d). `ClockBoard.cpp` defines the board
/// hook `nros_board_wall_clock_ns`, answering one fixed instant; this component's 1 s timer
/// prints what the wall clock then reads:
/// `clock-board: wall_ns=<ns> synced=<0|1> source=<id> steps=<n>` -- the wall clock through
/// `rclcpp::Clock(RCL_SYSTEM_TIME).now()`, then `rclcpp::Clock::wall_clock_state()`.
class ClockBoard {
    ::rclcpp::Timer timer_;

    void on_tick(); // bound via &ClockBoard::on_tick

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

} // namespace clock_board_pkg
