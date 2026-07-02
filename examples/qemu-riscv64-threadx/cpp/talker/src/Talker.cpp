/// @file Talker.cpp
/// @brief QEMU RISC-V ThreadX C++ talker — typed component (RFC-0043). Real
///        `on_tick` body bound by identity; no callback name, no interpreter.

#include "Talker.hpp"

#include <cstdio>

namespace riscv64_threadx_cpp_talker {

void Talker::on_tick() {
    // Pre-increment so the first payload is "Hello World: 1", matching the
    // official ROS 2 demo talker.
    ++count_;
    // Global printf/snprintf/setvbuf (not std::) — picolibc's <cstdio> on the
    // bare-metal riscv64 toolchain declares them in the global namespace
    // only. Portable to glibc/newlib too.
    char payload[64];
    snprintf(payload, sizeof(payload), "Hello World: %d", count_);
    std_msgs::msg::String m;
    m.data = payload;
    if (pub_.publish(m).ok()) {
        printf("Publishing: '%s'\n", m.data.c_str());
    }
}

::nros::Result Talker::configure(::nros::Node& node) {
    setvbuf(stdout, nullptr, _IONBF, 0);
    ::nros::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    // Member-fn-pointer-as-template-param → no-alloc trampoline; `this` is ctx.
    return ::nros::bind_timer<Talker, &Talker::on_tick>(node, timer_, 1000, this);
}

} // namespace riscv64_threadx_cpp_talker
