/// @file service_server.cpp
/// @brief C++ AddTwoInts service server for the PURE BARE-METAL MPS2-AN385.
///
/// The same program as `examples/mps2-an385-freertos/cpp/service-server/src/main.cpp`.
/// What differs is having no RTOS (issue 1512) — see the `talker` sibling's
/// header: no libc or C++ runtime (`-ffreestanding`) and no `printf`, so log
/// lines are built by hand and emitted through `nros_log_emit_at`.

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

} // namespace

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    rclcpp::Node node;
    rclcpp::PollService<example_interfaces::srv::AddTwoInts> srv;

    if (!rclcpp::init_in(NROS_ENTRY_LOCATOR, static_cast<uint8_t>(NROS_ENTRY_DOMAIN_ID)).ok()) {
        emit(nros_log_default_logger(), NROS_LOG_SEVERITY_ERROR, "rclcpp::init failed", __FILE__,
             __LINE__);
        return 1;
    }
    if (!rclcpp::create_node(node, "add_two_ints_server").ok()) {
        emit(nros_log_default_logger(), NROS_LOG_SEVERITY_ERROR, "create_node failed", __FILE__,
             __LINE__);
        return 1;
    }
    nros_logger_t logger = node.get_logger();
    if (!node.create_service(srv, "/add_two_ints").ok()) {
        emit(logger, NROS_LOG_SEVERITY_ERROR, "create_service failed", __FILE__, __LINE__);
        return 1;
    }
    emit(logger, NROS_LOG_SEVERITY_INFO, "Waiting for service requests", __FILE__, __LINE__);

    while (rclcpp::ok()) {
        rclcpp::spin_once(100);

        example_interfaces::srv::AddTwoInts::Request req;
        int64_t seq_id = 0;
        while (srv.take_request(req, seq_id)) {
            example_interfaces::srv::AddTwoInts::Response resp;
            resp.sum = req.a + req.b;

            char line[96];
            size_t len = append_str(line, sizeof(line), 0u, "Incoming request a: ");
            len = append_i64(line, sizeof(line), len, req.a);
            len = append_str(line, sizeof(line), len, " b: ");
            len = append_i64(line, sizeof(line), len, req.b);
            line[len] = '\0';
            emit(logger, NROS_LOG_SEVERITY_INFO, line, __FILE__, __LINE__);

            if (!srv.send_response(seq_id, resp).ok()) {
                emit(logger, NROS_LOG_SEVERITY_ERROR, "send_response failed", __FILE__, __LINE__);
            }
        }
    }
    rclcpp::shutdown();
    return 0;
}

NROS_APP_MAIN_REGISTER()
