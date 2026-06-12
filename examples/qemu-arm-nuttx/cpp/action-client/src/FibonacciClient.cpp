/// @file FibonacciClient.cpp
/// @brief NuttX C++ Fibonacci action client — typed CALLBACK component.

#include "FibonacciClient.hpp"

#include <cstdio>

namespace nuttx_cpp_action_client {

static uint32_t read_u32_le(const uint8_t* p) {
    return static_cast<uint32_t>(p[0]) | (static_cast<uint32_t>(p[1]) << 8) |
           (static_cast<uint32_t>(p[2]) << 16) | (static_cast<uint32_t>(p[3]) << 24);
}

static void write_u32_le(uint8_t* p, uint32_t v) {
    p[0] = static_cast<uint8_t>(v);
    p[1] = static_cast<uint8_t>(v >> 8);
    p[2] = static_cast<uint8_t>(v >> 16);
    p[3] = static_cast<uint8_t>(v >> 24);
}

void FibonacciClient::on_goal_response(bool accepted, const uint8_t goal_id[16]) {
    if (accepted) {
        std::printf("Goal accepted by server\n");
        // Request the result asynchronously; it arrives in on_result.
        nros_cpp_action_client_get_result_async(client_.bytes,
                                                reinterpret_cast<const uint8_t(*)[16]>(goal_id));
    } else {
        std::printf("Goal rejected by server\n");
    }
}

void FibonacciClient::on_feedback(const uint8_t* /*goal_id*/, const uint8_t* /*data*/,
                                  size_t /*len*/) {
    // Fibonacci feedback (partial_sequence) is not asserted by this example.
}

void FibonacciClient::on_result(const uint8_t* /*goal_id*/, int32_t status, const uint8_t* data,
                                size_t len) {
    uint32_t count = (len >= 8) ? read_u32_le(data + 4) : 0;
    std::printf("Result (status=%d): %u terms\n", static_cast<int>(status),
                static_cast<unsigned>(count));
    std::printf("Action completed successfully\n");
}

::nros::Result FibonacciClient::configure(::nros::Node& node) {
    // Unbuffered stdout — the callback prints only on transitions, so a
    // full-buffered console would swallow them when the harness kills the QEMU.
    std::setvbuf(stdout, nullptr, _IONBF, 0);

    ::nros::Result r =
        ::nros::bind_action_client<FibonacciClient, &FibonacciClient::on_goal_response,
                                   &FibonacciClient::on_feedback, &FibonacciClient::on_result>(
            node, client_, poll_timer_, "/fibonacci", "example_interfaces/action/Fibonacci", this);
    if (!r.ok()) return r;

    // Send one goal (async — the acceptance arrives in on_goal_response).
    uint8_t goal[8];
    goal[0] = 0x00;
    goal[1] = 0x01;
    goal[2] = 0x00;
    goal[3] = 0x00;
    write_u32_le(goal + 4, static_cast<uint32_t>(order_));
    uint8_t goal_id[16];
    nros_cpp_action_client_send_goal_async(client_.bytes, goal, sizeof(goal), &goal_id);
    std::printf("Goal sent: order=%d\n", static_cast<int>(order_));
    return ::nros::Result();
}

} // namespace nuttx_cpp_action_client
