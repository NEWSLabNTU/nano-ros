/// @file action_client.cpp
/// @brief C++ Fibonacci action client for the PURE BARE-METAL MPS2-AN385 —
///        sends one goal (order 10), logs feedback and the result.
///
/// The same program as `examples/mps2-an385-freertos/cpp/action-client/src/main.cpp`,
/// minus the `NROS_TEST_GOAL_ORDER` getenv (there is no environment on bare
/// metal). What differs is having no RTOS (issue 1512) — see the `talker`
/// sibling's header: no libc or C++ runtime and no `printf`.

#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <nros/app_main.h>
#include <nros/log.h>
#include <nros/nros.hpp>

#include "example_interfaces.hpp"

// Backend registration (see the talker sibling).
extern "C" int nros_rmw_zenoh_register(void);

extern "C" void nros_app_register_backends(void) {
    (void)nros_rmw_zenoh_register();
}

namespace {

using Fibonacci = example_interfaces::action::Fibonacci;

size_t append_str(char* out, size_t cap, size_t pos, const char* text) {
    while (*text != '\0' && pos + 1u < cap) {
        out[pos++] = *text++;
    }
    return pos;
}

size_t append_i64(char* out, size_t cap, size_t pos, int64_t value) {
    uint64_t mag = value < 0 ? 0u - static_cast<uint64_t>(value) : static_cast<uint64_t>(value);
    if (value < 0 && pos + 1u < cap) {
        out[pos++] = '-';
    }
    char digits[20];
    size_t n = 0;
    do {
        digits[n++] = static_cast<char>('0' + (mag % 10u));
        mag /= 10u;
    } while (mag != 0u && n < sizeof(digits));
    while (n > 0u && pos + 1u < cap) {
        out[pos++] = digits[--n];
    }
    return pos;
}

void emit(nros_logger_t logger, nros_log_severity_t severity, const char* text, const char* file,
          uint32_t line) {
    nros_log_emit_at(logger, severity, text, strlen(text), file, line);
}

/// `<prefix>[a, b, c]` — the sequence formatting both log lines share.
template <typename Seq>
void emit_sequence(nros_logger_t logger, const char* prefix, const Seq& seq, uint32_t line_no) {
    char line[384];
    size_t len = append_str(line, sizeof(line), 0u, prefix);
    len = append_str(line, sizeof(line), len, "[");
    for (uint32_t k = 0; k < seq.length(); k++) {
        if (k > 0) {
            len = append_str(line, sizeof(line), len, ", ");
        }
        len = append_i64(line, sizeof(line), len, seq[k]);
    }
    len = append_str(line, sizeof(line), len, "]");
    line[len] = '\0';
    emit(logger, NROS_LOG_SEVERITY_INFO, line, __FILE__, line_no);
}

} // namespace

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    rclcpp::Node node;
    rclcpp_action::Client<Fibonacci> client;

    if (!rclcpp::init_in(NROS_ENTRY_LOCATOR, static_cast<uint8_t>(NROS_ENTRY_DOMAIN_ID)).ok()) {
        emit(nros_log_default_logger(), NROS_LOG_SEVERITY_ERROR, "rclcpp::init failed", __FILE__,
             __LINE__);
        return 1;
    }
    if (!rclcpp::create_node(node, "fibonacci_action_client").ok()) {
        emit(nros_log_default_logger(), NROS_LOG_SEVERITY_ERROR, "create_node failed", __FILE__,
             __LINE__);
        return 1;
    }
    nros_logger_t logger = node.get_logger();
    if (!node.create_action_client(client, "/fibonacci").ok()) {
        emit(logger, NROS_LOG_SEVERITY_ERROR, "create_action_client failed", __FILE__, __LINE__);
        return 1;
    }

    rclcpp::Result ret = client.wait_for_action_server(10000);
    if (!ret.ok() && ret.code() != rclcpp::ErrorCode::Unsupported) {
        emit(logger, NROS_LOG_SEVERITY_ERROR, "action server did not appear within 10s", __FILE__,
             __LINE__);
        rclcpp::shutdown();
        return 2;
    }

    emit(logger, NROS_LOG_SEVERITY_INFO, "Sending goal", __FILE__, __LINE__);
    Fibonacci::Goal goal;
    goal.order = 10;
    uint8_t goal_id[16];
    ret = client.send_goal(goal, goal_id);
    if (!ret.ok()) {
        emit(logger, NROS_LOG_SEVERITY_ERROR,
             ret.code() == rclcpp::ErrorCode::Rejected ? "goal was rejected by server"
                                                       : "failed to send goal",
             __FILE__, __LINE__);
        rclcpp::shutdown();
        return 2;
    }
    emit(logger, NROS_LOG_SEVERITY_INFO, "Goal accepted by server, waiting for result", __FILE__,
         __LINE__);

    auto& feedback = client.feedback_stream();
    for (int i = 0; i < 20; i++) {
        rclcpp::spin_once(100);
        Fibonacci::Feedback fb;
        while (feedback.try_next(fb).ok()) {
            emit_sequence(logger, "Next number in sequence received: ", fb.sequence, __LINE__);
        }
    }

    Fibonacci::Result result;
    ret = client.get_result(goal_id, result);
    if (!ret.ok()) {
        emit(logger, NROS_LOG_SEVERITY_ERROR, "failed to get result", __FILE__, __LINE__);
        rclcpp::shutdown();
        return 1;
    }
    emit_sequence(logger, "Result received: ", result.sequence, __LINE__);
    rclcpp::shutdown();
    return 0;
}

NROS_APP_MAIN_REGISTER()
