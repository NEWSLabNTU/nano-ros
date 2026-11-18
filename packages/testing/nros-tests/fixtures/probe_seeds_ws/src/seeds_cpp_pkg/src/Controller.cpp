// Issue 2640 -- see Controller.hpp. The host metadata probe seeds the
// launch values before it constructs the node, so its sidecar records the
// timer and all 41 parameters.

#include "seeds_cpp_pkg/Controller.hpp"

#include <stdint.h>

namespace seeds_cpp_pkg {

namespace {
constexpr int kGains = 40;
const char* const kGainNames[kGains] = {
    "gain_00", "gain_01", "gain_02", "gain_03", "gain_04", "gain_05", "gain_06", "gain_07",
    "gain_08", "gain_09", "gain_10", "gain_11", "gain_12", "gain_13", "gain_14", "gain_15",
    "gain_16", "gain_17", "gain_18", "gain_19", "gain_20", "gain_21", "gain_22", "gain_23",
    "gain_24", "gain_25", "gain_26", "gain_27", "gain_28", "gain_29", "gain_30", "gain_31",
    "gain_32", "gain_33", "gain_34", "gain_35", "gain_36", "gain_37", "gain_38", "gain_39",
};
} // namespace

Controller::Controller(::rclcpp::NodeHandle h) : ::rclcpp::NodeWithTimers<1>(h, "seeds_cpp") {
    const int64_t period_ms = declare_parameter<int64_t>("ctrl_period_ms");
    for (int i = 0; i < kGains; i++) {
        (void)declare_parameter<int64_t>(kGainNames[i], i);
    }
    if (period_ms > 0) {
        create_wall_timer_in<Controller, &Controller::on_timer>(static_cast<uint64_t>(period_ms));
    }
}

} // namespace seeds_cpp_pkg
