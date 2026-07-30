// FVP AEMv8-R Cortex-A/R cyclonedds talker — typed component (RFC-0043 /
// phase-244.C2.1). A timer member publishes the official ROS 2 demo payload on `/chatter`
// via a typed `Publisher<String>`. The Zephyr typed carrier
// (`zephyr_entry_main_typed.cpp.in`) constructs this object + calls
// `configure(node)` and runs `ZephyrBoard::run_components`. Replaces the legacy
// Phase-117 imperative `main.cpp` (`nros::init`/`create_node`/`while`/`k_sleep`).
#ifndef NROS_ZEPHYR_AEMV8R_CYCLONEDDS_TALKER_HPP
#define NROS_ZEPHYR_AEMV8R_CYCLONEDDS_TALKER_HPP

#include <nros/component.hpp>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

namespace nros_zephyr_aemv8r_cyclonedds_talker {

class Talker {
    ::nros::Publisher<std_msgs::msg::String> pub_;
    ::nros::Timer timer_;
    int count_ = 0;

    void on_tick(); // real body, bound by identity

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace nros_zephyr_aemv8r_cyclonedds_talker

#endif // NROS_ZEPHYR_AEMV8R_CYCLONEDDS_TALKER_HPP
