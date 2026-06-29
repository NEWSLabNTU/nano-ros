#pragma once

#include <cstddef>
#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

namespace qos_listener_pkg {

/// QosListener — a stateful typed component (RFC-0043). `configure` binds the
/// member `on_raw` as a raw zero-copy subscription on `/chatter` with a QoS
/// profile that MATCHES the talker's publisher (RELIABLE + TRANSIENT_LOCAL +
/// KEEP_LAST(10), built via the `nros::QoS` builder). Matching the per-entity QoS
/// contract is what lets the QoS-tagged endpoints connect. The member decodes the
/// CDR-encoded Int32 and prints `Received: N`.
class QosListener {
    int recv_ = 0;

    void on_raw(const uint8_t* data, size_t len); // real body; bound by identity

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace qos_listener_pkg
