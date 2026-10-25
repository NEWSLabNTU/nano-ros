/*
 * NEGATIVE probe — issue 2024, `rclcpp::ParameterEventHandler` WITHOUT the
 * hosted surface.
 *
 * A hosted build has it (`parameter_event_handler_runtime.cpp`). A
 * freestanding build has no `std::function`, `std::string` or allocator for
 * its callbacks, handles and decoded parameters, so constructing one is a
 * `static_assert` naming what works there.
 *
 * `just check cpp` compiles this WITHOUT `NROS_CPP_STD` and requires it to FAIL
 * with `REFUSED by nano-ros` AND `issue 2024` in the text.
 */

#include <rclcpp/rclcpp.hpp>

int parameter_event_handler_freestanding_probe(rclcpp::Node* node);
int parameter_event_handler_freestanding_probe(rclcpp::Node* node) {
    rclcpp::ParameterEventHandler handler(node);
    (void)handler;
    return 0;
}
