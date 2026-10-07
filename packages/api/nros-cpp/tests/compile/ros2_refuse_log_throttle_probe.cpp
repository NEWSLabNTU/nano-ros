/*
 * NEGATIVE probe — phase-417 W3.a (issue 1019), the throttle half.
 *
 * `RCLCPP_*_THROTTLE(logger, clock, period_ms, fmt, ...)` expanded to the plain
 * `RCLCPP_*` macro with `clock` and `period_ms` left UNEVALUATED. So a 1 Hz
 * throttle logged at loop rate — the exact opposite of what the call asks for —
 * and a side-effecting clock expression was dropped entirely. "Compiles and
 * differs", which RFC-0089 forbids.
 *
 * C does have a throttle now (`NROS_LOG_*_THROTTLE`, phase-417 W4.d), but it
 * measures its window on nros_log's own clock, and the upstream macro measures
 * it on the `clock` argument. Forwarding would drop that argument and compile
 * and differ, so this stays REFUSE-LOUD (issue 1302 corrected the reason).
 *
 * `just check cpp` requires this TU to FAIL. The STREAM half of issue 1019 is
 * the opposite disposition — it is IMPLEMENTED now, and the POSITIVE probe
 * (`ros2_api_adoption_stage2.cpp`) proves the message reaches the sink instead
 * of being discarded. Compile that one first.
 */

#include <nros/nros.hpp>

int ros2_refuse_log_throttle_probe(nros::Clock& clock);
int ros2_refuse_log_throttle_probe(nros::Clock& clock) {
    auto logger = rclcpp::get_logger("probe");
    RCLCPP_INFO_THROTTLE(logger, clock, 1000, "once per second, allegedly: %d", 1);
    RCLCPP_WARN_THROTTLE(logger, clock, 1000, "warn %d", 2);
    RCLCPP_ERROR_THROTTLE(logger, clock, 1000, "error %d", 3);
    RCLCPP_DEBUG_THROTTLE(logger, clock, 1000, "debug %d", 4);
    RCLCPP_FATAL_THROTTLE(logger, clock, 1000, "fatal %d", 5);
    return 0;
}
