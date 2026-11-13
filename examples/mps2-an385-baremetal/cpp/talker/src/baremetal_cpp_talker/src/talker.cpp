// The C++ talker component (phase-553 W5): an ordinary typed component that
// names no board. The entry generated for the bringup's image constructs it
// and calls `configure` on the image's executor.
#include "baremetal_cpp_talker/Talker.hpp"

#include <cstdio>

namespace baremetal_cpp_talker {

void Talker::on_tick() {
    ++count_;
    char payload[64];
    std::snprintf(payload, sizeof(payload), "Hello World: %d", count_);
    std_msgs::msg::String msg;
    msg.data = payload;
    if (pub_.publish(msg).ok()) {
        std::printf("Publishing: '%s'\n", msg.data.c_str());
    }
}

::rclcpp::Result Talker::configure(::rclcpp::Node& node) {
    // `::setvbuf` (C global), not `std::setvbuf`: Zephyr's picolibc lacks the std:: name.
    ::setvbuf(stdout, nullptr, _IONBF, 0);
    ::rclcpp::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    // Member-function pointer as a template parameter: a no-alloc trampoline, `this` is the
    // context.
    return node.create_wall_timer<Talker, &Talker::on_tick>(timer_, 1000, this);
}

} // namespace baremetal_cpp_talker
