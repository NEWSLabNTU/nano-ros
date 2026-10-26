// SPDX-License-Identifier: Apache-2.0
#ifndef WORKSPACE_PORT_CORE__SEQUENCE_HPP_
#define WORKSPACE_PORT_CORE__SEQUENCE_HPP_

#include <cstdint>
#include <mutex>

// A message whose own header includes another package's
// (`std_msgs/Header` -> `builtin_interfaces/Time`).
#include "std_msgs/msg/header.hpp"

namespace workspace_port_core {

/// A counter that may be read from more than one thread, so it locks.
class Sequence {
  public:
    /// The next number, starting from 0.
    int32_t next();
    /// The stamp of the last `next()`; its `frame_id` names the sequence.
    std_msgs::msg::Header last() const;

  private:
    mutable std::mutex mutex_;
    int32_t value_ = 0;
    std_msgs::msg::Header last_;
};

} // namespace workspace_port_core

#endif // WORKSPACE_PORT_CORE__SEQUENCE_HPP_
