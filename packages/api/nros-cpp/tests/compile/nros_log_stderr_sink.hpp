// A stand-in for `nros_log`'s two C entry points, for the runtime probes that
// link no nano-ros archive — issue 1303.
//
// The runtime refusals (`rclcpp::detail::abort_failed_create`, the
// `--ros-args` refusal in `rclcpp::init`) emit through `nros_log` so that they
// reach `printk` / `LOG_ERR` on an RTOS. A probe linked with
// `--unresolved-symbols=ignore-all` may leave only DATA unresolved (the issue-0360
// variant anchors): an unresolved FUNCTION makes the loader refuse the binary
// (`unexpected PLT reloc type`). So a probe that reaches a refusal includes this
// header, once, in its one TU.
//
// It writes each record to fd 2, one per line, because that is what the probes
// assert on and what the real host sink does too. Whether the REAL sink reaches
// stderr is `ros2_loudness_runtime.cpp`'s question, which links `libnros_cpp.a`;
// whether a freestanding build reaches `nros_log` at all is
// `refusal_reaches_nros_log.cpp`'s.
//
// It also enforces the half of issue 1303 that a route check cannot see: a
// record longer than `rclcpp::detail::RUNTIME_REFUSAL_MAX` does not survive
// `nros_log`'s format buffer (the body is dropped and a lone ellipsis is
// printed), so this sink writes `NROS_LOG_STDERR_SINK_OVERSIZE` in its place,
// and a probe asserts the marker is absent. Before issue 1303 the failed-create
// refusal was ONE record of over 1180 bytes; on a freestanding build
// `nros_log_emit_fmt_at` cut it to 255, which is still past this bound, so its
// body never reached the console.

#ifndef NROS_TESTS_NROS_LOG_STDERR_SINK_HPP
#define NROS_TESTS_NROS_LOG_STDERR_SINK_HPP

#include <unistd.h>

#include <nros/log.h>
#include <nros/log.hpp>

extern "C" {

nros_logger_t nros_log_default_logger(void) {
    static const char tag = 0;
    return &tag;
}

void nros_log_emit_at(nros_logger_t, nros_log_severity_t, const char* message, size_t message_len,
                      const char*, uint32_t) {
    if (message_len > ::rclcpp::detail::RUNTIME_REFUSAL_MAX) {
        static const char oversize[] = "NROS_LOG_STDERR_SINK_OVERSIZE";
        (void)::write(2, oversize, sizeof(oversize) - 1);
    } else if (message != nullptr && message_len > 0) {
        (void)::write(2, message, message_len);
    }
    (void)::write(2, "\n", 1);
}

} // extern "C"

#endif // NROS_TESTS_NROS_LOG_STDERR_SINK_HPP
