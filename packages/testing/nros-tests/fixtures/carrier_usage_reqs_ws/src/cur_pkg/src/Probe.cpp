#include "cur_pkg/Probe.hpp"

#include <cstdio>

#include <cur_vendor/answer.hpp>

namespace cur_pkg {

Probe::Probe(::rclcpp::NodeHandle h) : ::rclcpp::NodeWithTimers<1>(h, "cur_probe") {
    create_wall_timer_in<Probe, &Probe::on_timer>(cur_pkg_node::kProbePeriodMs);
}

void Probe::on_timer() {
    if (done_) {
        return;
    }
    done_ = true;
    std::printf("CARRIER_USAGE_REQS answer=%d\n", cur_vendor::answer());
    std::fflush(stdout);
}

} // namespace cur_pkg
