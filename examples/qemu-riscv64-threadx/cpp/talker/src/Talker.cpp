/// @file Talker.cpp
/// @brief QEMU RISC-V ThreadX C++ talker — typed component (RFC-0043). Real
///        `on_tick` body bound by identity; no callback name, no interpreter.

#include "Talker.hpp"

#include <cstdio>

namespace riscv64_threadx_cpp_talker {

void Talker::on_tick() {
    std_msgs::msg::Int32 m;
    m.data = count_++;
    if (pub_.publish(m).ok()) {
        // Global printf/setvbuf (not std::) — picolibc's <cstdio> on the
        // bare-metal riscv64 toolchain declares them in the global namespace
        // only. Portable to glibc/newlib too.
        printf("Published: %d\n", m.data);
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
