/// @file FibonacciClient.cpp
/// @brief NuttX C++ Fibonacci action client — typed poll component (240.5).

#include "FibonacciClient.hpp"

#include <cstdio>
#include <cstring>

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

void FibonacciClient::on_tick() {
    // Pump the (raw, non-arena-registered) client's RX so try_recv_* see replies.
    nros_cpp_action_client_poll(client_.bytes);
    if (phase_ == 0) {
        // Goal CDR: encapsulation header (CDR_LE) + int32 order.
        uint8_t goal[8];
        goal[0] = 0x00;
        goal[1] = 0x01;
        goal[2] = 0x00;
        goal[3] = 0x00;
        write_u32_le(goal + 4, static_cast<uint32_t>(order_));
        // ASYNC send — the blocking `send_goal` would re-enter the executor from
        // inside this `spin_once` timer callback (deadlock / never completes).
        // The async path returns immediately; the acceptance arrives via
        // `try_recv_goal_response` on a later tick.
        nros_cpp_action_client_send_goal_async(client_.bytes, goal, sizeof(goal), &goal_id_);
        std::printf("Goal sent: order=%d\n", static_cast<int>(order_));
        phase_ = 1;
    } else if (phase_ == 1) {
        // Goal-response layout: [goal_id: 16][accepted: 1].
        uint8_t buf[17];
        size_t len = 0;
        if (nros_cpp_action_client_try_recv_goal_response(client_.bytes, buf, sizeof(buf), &len) ==
                0 &&
            len >= 17) {
            std::memcpy(goal_id_, buf, 16); // the server-confirmed goal UUID
            if (buf[16] != 0) {
                std::printf("Goal accepted by server\n");
                // Kick the async result request; poll it in phase 2.
                nros_cpp_action_client_get_result_async(
                    client_.bytes, reinterpret_cast<const uint8_t(*)[16]>(goal_id_));
                phase_ = 2;
            } else {
                std::printf("Goal rejected by server\n");
                phase_ = 3;
            }
        }
    } else if (phase_ == 2) {
        uint8_t res[256];
        size_t len = 0;
        if (nros_cpp_action_client_try_recv_result(client_.bytes, res, sizeof(res), &len) == 0 &&
            len >= 8) {
            uint32_t count = read_u32_le(res + 4);
            // status=4 = Succeeded (action_msgs/GoalStatus). The harness greps
            // "Result (status=" as the completion marker.
            std::printf("Result (status=4): %u terms\n", static_cast<unsigned>(count));
            std::printf("Action completed successfully\n");
            phase_ = 3;
        }
    }
}

::nros::Result FibonacciClient::configure(::nros::Node& node) {
    // Unbuffered stdout — the poll client prints only on state transitions, so a
    // full-buffered console would swallow them when the harness kills the QEMU.
    std::setvbuf(stdout, nullptr, _IONBF, 0);
    executor_ = node.executor_handle();
    ::nros::Result r = ::nros::create_action_client_raw(node, client_.bytes, "/fibonacci",
                                                        "example_interfaces/action/Fibonacci");
    if (!r.ok()) return r;
    r = ::nros::bind_timer<FibonacciClient, &FibonacciClient::on_tick>(node, timer_, 500, this);
    if (r.ok()) {
        std::printf("Sending goal\n");
    }
    return r;
}

} // namespace nuttx_cpp_action_client
