/// @file action_server.cpp
/// @brief C++ Fibonacci action server for the PURE BARE-METAL MPS2-AN385.
///
/// The same program as `examples/mps2-an385-freertos/cpp/action-server/src/main.cpp`.
/// What differs is having no RTOS (issue 1512) — see the `talker` sibling's
/// header: no libc or C++ runtime and no `printf`.

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

struct ServerState {
    rclcpp_action::Server<Fibonacci>* srv;
    nros_logger_t logger;
    uint32_t goal_count;
};

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

rclcpp_action::GoalResponse on_goal(const uint8_t uuid[16], const Fibonacci::Goal& goal,
                                    void* ctx) {
    ServerState* state = static_cast<ServerState*>(ctx);

    char line[64];
    size_t len = append_str(line, sizeof(line), 0u, "Received goal request with order ");
    len = append_i64(line, sizeof(line), len, goal.order);
    line[len] = '\0';
    emit(state->logger, NROS_LOG_SEVERITY_INFO, line, __FILE__, __LINE__);

    if (goal.order < 0 || goal.order >= 64) {
        emit(state->logger, NROS_LOG_SEVERITY_INFO, "Goal rejected: order out of range", __FILE__,
             __LINE__);
        return rclcpp_action::GoalResponse::REJECT;
    }
    state->goal_count++;
    emit(state->logger, NROS_LOG_SEVERITY_INFO, "Executing goal", __FILE__, __LINE__);

    int32_t a = 0;
    int32_t b = 1;
    Fibonacci::Result result;
    for (int32_t i = 0; i <= goal.order && i < 64; i++) {
        result.sequence.push_back(a);
        if (i > 0 && (i % 3 == 0 || i == goal.order)) {
            Fibonacci::Feedback fb;
            for (uint32_t k = 0; k < result.sequence.length(); k++) {
                fb.sequence.push_back(result.sequence[k]);
            }
            state->srv->publish_feedback(uuid, fb);
            emit(state->logger, NROS_LOG_SEVERITY_INFO, "Publish feedback", __FILE__, __LINE__);
        }
        int32_t next = a + b;
        a = b;
        b = next;
    }
    if (state->srv->complete_goal(uuid, result).ok()) {
        emit(state->logger, NROS_LOG_SEVERITY_INFO, "Goal succeeded", __FILE__, __LINE__);
    }
    return rclcpp_action::GoalResponse::ACCEPT_AND_EXECUTE;
}

} // namespace

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    rclcpp::Node node;
    rclcpp_action::Server<Fibonacci> srv;

    if (!rclcpp::init_in(NROS_ENTRY_LOCATOR, static_cast<uint8_t>(NROS_ENTRY_DOMAIN_ID)).ok()) {
        emit(nros_log_default_logger(), NROS_LOG_SEVERITY_ERROR, "rclcpp::init failed", __FILE__,
             __LINE__);
        return 1;
    }
    if (!rclcpp::create_node(node, "fibonacci_action_server").ok()) {
        emit(nros_log_default_logger(), NROS_LOG_SEVERITY_ERROR, "create_node failed", __FILE__,
             __LINE__);
        return 1;
    }
    ServerState state = {&srv, node.get_logger(), 0u};
    if (!node.create_action_server(srv, "/fibonacci").ok()) {
        emit(state.logger, NROS_LOG_SEVERITY_ERROR, "create_action_server failed", __FILE__,
             __LINE__);
        return 1;
    }
    srv.set_goal_callback_with_ctx(on_goal, &state);
    emit(state.logger, NROS_LOG_SEVERITY_INFO, "Waiting for action goals", __FILE__, __LINE__);

    while (rclcpp::ok()) {
        rclcpp::spin_once(100);
    }
    rclcpp::shutdown();
    return 0;
}

NROS_APP_MAIN_REGISTER()
