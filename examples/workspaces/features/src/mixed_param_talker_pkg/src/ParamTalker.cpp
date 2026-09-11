// ParamTalker — phase-426 W5's acceptance: one node, two languages, one store.
//
// C++ declares `publish_period_ms` (adopting the launch `<param>` seed of 250)
// and reads `scale`, which the C translation unit beside this one declared.
// C reads `publish_period_ms` back on every tick. The published value is
// `publish_period_ms * scale`, so the number a listener sees is only right if
// both crossings landed in the same `nros_params::ParameterServer`.

#include "mixed_param_talker_pkg/ParamTalker.hpp"

#include <cstdio>

#include "../src/param_probe.h"

namespace mixed_param_talker_pkg {

void ParamTalker::on_tick() {
    // The LIVE read, through C. `ros2 param set /mixed_param_talker
    // publish_period_ms N` moves this on the next tick, which is what makes it
    // a read of the store rather than of a value copied at configure time.
    int64_t period = -1;
    (void)mixed_param_c_read_period(node_handle_, &period);

    std_msgs::msg::Int32 m;
    m.data = static_cast<int32_t>(period * static_cast<int64_t>(scale_));
    if (pub_.publish(m).ok()) {
        std::printf("Published: %d\n", m.data);
    }
}

::rclcpp::Result ParamTalker::configure(::rclcpp::Node& node) {
    ::setvbuf(stdout, nullptr, _IONBF, 0);
    node_handle_ = node.ffi_handle();
    if (node_handle_ == nullptr) {
        return ::rclcpp::Result(::nros::ErrorCode::NotInitialized);
    }

    // C++ SIDE: declare. The launch file seeds 250 before user code runs, so
    // this adopts rather than sets — upstream's contract, and the value the C
    // read below must come back with.
    const int64_t period = node.declare_parameter<int64_t>("publish_period_ms", 100);

    // C SIDE: declare. Nothing in C++ ever names 3.0.
    if (mixed_param_c_declare_scale(node_handle_, 3.0) != NROS_CPP_RET_OK) {
        return ::rclcpp::Result(::nros::ErrorCode::Error);
    }

    // C++ reads what C declared. Two stores answer `false` here.
    if (!node.get_parameter<double>("scale", scale_)) {
        std::fprintf(stderr, "[cpp] `scale` (declared in C) is not visible to C++ — "
                             "the two languages are not sharing a store\n");
        return ::rclcpp::Result(::nros::ErrorCode::NotFound);
    }

    // And C reads what C++ declared, once, at configure — so a broken crossing
    // fails the BOOT rather than showing up as a wrong number on the wire.
    int64_t seen_by_c = -1;
    if (mixed_param_c_read_period(node_handle_, &seen_by_c) != NROS_CPP_RET_OK ||
        seen_by_c != period) {
        std::fprintf(stderr, "[cpp] C read publish_period_ms as %lld, C++ declared %lld\n",
                     static_cast<long long>(seen_by_c), static_cast<long long>(period));
        return ::rclcpp::Result(::nros::ErrorCode::Error);
    }
    std::printf("Crossed: cpp declared publish_period_ms=%lld, c read %lld; "
                "c declared scale=%.1f, cpp read %.1f\n",
                static_cast<long long>(period), static_cast<long long>(seen_by_c), 3.0, scale_);

    ::rclcpp::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    return node.create_wall_timer<ParamTalker, &ParamTalker::on_tick>(timer_, 500, this);
}

} // namespace mixed_param_talker_pkg
