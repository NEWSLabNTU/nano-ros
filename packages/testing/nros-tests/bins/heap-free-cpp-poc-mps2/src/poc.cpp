/// @file poc.cpp
/// @brief Issue 2131 -- the C++ program of the heap-free-tier image.
///
/// Ordinary nano-ros C++: init, a node, a parameter declared, set and read
/// back, a timer, a spin. No RMW is linked, so `rclcpp::init_in()` is refused
/// at run time and the program returns 0, which `src/main.rs` reports as the
/// success marker. Everything after the init stays in the image because the
/// compiler cannot know the init's outcome, and that is what the link gate
/// reads. Returning 2 means the init unexpectedly succeeded.
///
/// Freestanding, as on every bare-metal C++ image: no libc, no C++ runtime, no
/// exceptions, no RTTI (build.rs). Handles are locals, because `cortex-m-rt`
/// runs no C++ static constructors.

#include <stdint.h>

#include <rclcpp/rclcpp.hpp>

namespace {

void on_tick(void* context) {
    uint32_t* ticks = static_cast<uint32_t*>(context);
    *ticks += 1u;
}

} // namespace

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    rclcpp::Node node;
    rclcpp::Timer timer;
    uint32_t ticks = 0u;

    if (!rclcpp::init_in().ok()) {
        return 0;
    }
    if (!rclcpp::create_node(node, "heap_free_cpp_poc").ok()) {
        return 3;
    }
    // A parameter declared, set and read back through the node. The store is
    // carved from the executor backing (`NROS_PARAM_STORE`), not the heap.
    int64_t rate = node.declare_parameter<int64_t>("rate_hz", 7);
    if (!node.set_parameter<int64_t>("rate_hz", rate * 6).ok()) {
        return 4;
    }
    if (!node.get_parameter<int64_t>("rate_hz", rate)) {
        return 5;
    }
    if (!node.create_wall_timer(timer, 100, on_tick, &ticks).ok()) {
        return 6;
    }
    if (!rclcpp::spin_once(10).ok()) {
        return 8;
    }
    rclcpp::shutdown();
    return rate == 42 ? 2 : 7;
}
