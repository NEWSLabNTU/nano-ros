/// @file AddTwoIntsClient.cpp
/// @brief Zephyr C++ AddTwoInts service client — typed component.

#include "AddTwoIntsClient.hpp"

#include <cstdio>

namespace zephyr_cpp_service_client {

static int64_t read_i64_le(const uint8_t* p) {
    uint64_t v = 0;
    for (int i = 0; i < 8; ++i) {
        v |= static_cast<uint64_t>(p[i]) << (8 * i);
    }
    return static_cast<int64_t>(v);
}

static void write_i64_le(uint8_t* p, int64_t x) {
    uint64_t v = static_cast<uint64_t>(x);
    for (int i = 0; i < 8; ++i) {
        p[i] = static_cast<uint8_t>(v >> (8 * i));
    }
}

void AddTwoIntsClient::on_tick() {
    if (done_) {
        return;
    }
    if (!awaiting_) {
        // Request CDR: encapsulation header (CDR_LE) + int64 a + int64 b.
        uint8_t req[20];
        req[0] = 0x00;
        req[1] = 0x01;
        req[2] = 0x00;
        req[3] = 0x00;
        write_i64_le(req + 4, a_);
        write_i64_le(req + 12, b_);
        if (nros_cpp_service_client_send_request_seq(client_.bytes, req, sizeof(req), &seq_) == 0) {
            awaiting_ = true;
        }
        return;
    }
    uint8_t resp[64];
    size_t len = 0;
    int64_t seq = NROS_CPP_SEQUENCE_ID_UNKNOWN;
    // Issue 2033 — a reply naming another request than ours is stale: drop it.
    if (nros_cpp_service_client_take_response_seq(client_.bytes, resp, sizeof(resp), &len, &seq) ==
            0 &&
        len >= 12 &&
        (seq == seq_ || seq == NROS_CPP_SEQUENCE_ID_UNKNOWN ||
         seq_ == NROS_CPP_SEQUENCE_ID_UNKNOWN)) {
        int64_t sum = read_i64_le(resp + 4);
        std::printf("Result of add_two_ints: %lld\n", static_cast<long long>(sum));
        done_ = true;
    }
}

::rclcpp::Result AddTwoIntsClient::configure(::rclcpp::Node& node) {
    // Unbuffered stdout — a full-buffered console can swallow the final
    // line(s) when the harness kills the QEMU before a flush.
    // `::setvbuf` (global) not `std::setvbuf` — Zephyr's minimal libcpp/picolibc
    // `<cstdio>` declares it in the global namespace only.
    ::setvbuf(stdout, nullptr, _IONBF, 0);
    ::rclcpp::Result r = ::rclcpp::create_service_client_raw(node, client_.bytes, "/add_two_ints",
                                                             "example_interfaces/srv/AddTwoInts");
    if (!r.ok()) return r;
    r = node.create_wall_timer<AddTwoIntsClient, &AddTwoIntsClient::on_tick>(timer_, 1000, this);
    if (r.ok()) {
        std::printf("Sending request\n");
    }
    return r;
}

} // namespace zephyr_cpp_service_client
