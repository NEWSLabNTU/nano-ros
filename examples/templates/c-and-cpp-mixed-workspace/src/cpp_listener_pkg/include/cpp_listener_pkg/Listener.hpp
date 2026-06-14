#pragma once

#include <cstddef>
#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

namespace cpp_listener_pkg {

/// Listener — a stateful component (RFC-0043). `configure` binds the member
/// `on_raw` (by identity, no name) as a raw zero-copy subscription on
/// `/chatter`. The raw path borrows the wire bytes (no copy/deserialize); the
/// member decodes the CDR-encoded Int32 and counts receipts. Replaces the
/// legacy `register_node` + `record_callback_effect` declarative seam.
class Listener {
    int recv_ = 0;

    void on_raw(const uint8_t* data, size_t len); // real body; bound by identity

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace cpp_listener_pkg
