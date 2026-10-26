// SPDX-License-Identifier: Apache-2.0
#include "workspace_port_core/sequence.hpp"

namespace workspace_port_core {

int32_t Sequence::next() {
    std::lock_guard<std::mutex> lock(mutex_);
    last_.stamp.sec = value_;
    return value_++;
}

std_msgs::msg::Header Sequence::last() const {
    std::lock_guard<std::mutex> lock(mutex_);
    return last_;
}

} // namespace workspace_port_core
