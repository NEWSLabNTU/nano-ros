// NEGATIVE probe -- `rclcpp::Executor` is not movable (issue 3003).
//
// Its storage holds a context that points into itself, so a move that copies
// the bytes leaves the new executor reading the old, zeroed one. The move
// operations are deleted. `just check cpp` compiles this TU at C++14, the
// lowest standard any toolchain builds nros-cpp with, and requires it to FAIL
// with a diagnostic naming the deleted move. A TU that breaks for any other
// reason proves nothing.

#include <rclcpp/rclcpp.hpp>

#include <utility>

int executor_move_probe();
int executor_move_probe() {
    rclcpp::Executor a;
    rclcpp::Executor b = std::move(a);
    (void)b;
    return 0;
}
