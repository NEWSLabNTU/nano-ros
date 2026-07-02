/// @file ActionClient.cpp
/// @brief ThreadX-Linux C++ Fibonacci action client — typed CALLBACK component.

#include "ActionClient.hpp"

#include <cstdio>

namespace threadx_linux_cpp_action_client {

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

// Print an int32 sequence CDR payload (encap header + u32 length + N int32)
// as `<prefix>[0, 1, 1, ...]`.
static void print_sequence(const char* prefix, const uint8_t* data, size_t len) {
    uint32_t count = (len >= 8) ? read_u32_le(data + 4) : 0u;
    printf("%s[", prefix);
    for (uint32_t i = 0; i < count && static_cast<size_t>(8 + 4 * i + 4) <= len; ++i) {
        if (i > 0) {
            printf(", ");
        }
        printf("%d", static_cast<int>(static_cast<int32_t>(read_u32_le(data + 8 + 4 * i))));
    }
    printf("]\n");
}

void ActionClient::on_goal_response(bool accepted, const uint8_t goal_id[16]) {
    if (accepted) {
        printf("Goal accepted by server, waiting for result\n");
        nros_cpp_action_client_get_result_async(client_.bytes,
                                                reinterpret_cast<const uint8_t(*)[16]>(goal_id));
    } else {
        printf("Goal rejected by server\n");
    }
}

void ActionClient::on_feedback(const uint8_t* /*goal_id*/, const uint8_t* data, size_t len) {
    print_sequence("Next number in sequence received: ", data, len);
}

void ActionClient::on_result(const uint8_t* /*goal_id*/, int32_t /*status*/, const uint8_t* data,
                             size_t len) {
    print_sequence("Result received: ", data, len);
}

::nros::Result ActionClient::configure(::nros::Node& node) {
    setvbuf(stdout, nullptr, _IONBF, 0);

    ::nros::Result r =
        ::nros::bind_action_client<ActionClient, &ActionClient::on_goal_response,
                                   &ActionClient::on_feedback, &ActionClient::on_result>(
            node, client_, poll_timer_, "/fibonacci", "example_interfaces/action/Fibonacci", this);
    if (!r.ok()) return r;

    uint8_t goal[8];
    goal[0] = 0x00;
    goal[1] = 0x01;
    goal[2] = 0x00;
    goal[3] = 0x00;
    write_u32_le(goal + 4, static_cast<uint32_t>(order_));
    uint8_t goal_id[16];
    printf("Sending goal\n");
    nros_cpp_action_client_send_goal_async(client_.bytes, goal, sizeof(goal), &goal_id);
    return ::nros::Result();
}

} // namespace threadx_linux_cpp_action_client
