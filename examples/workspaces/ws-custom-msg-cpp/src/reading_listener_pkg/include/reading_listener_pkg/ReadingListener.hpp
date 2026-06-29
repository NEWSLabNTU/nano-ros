#pragma once

#include <cstddef>
#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>

namespace reading_listener_pkg {

/// ReadingListener — raw-CDR typed component (RFC-0043), the C++ projection of
/// ws-custom-msg-c's ReadingListener. `configure` binds `on_raw` as a raw
/// zero-copy subscription on `/reading`, carrying the workspace-local type name
/// as a string. The callback decodes the CDR-encoded `Reading` and prints the
/// `sequence` + `temperature` fields.
class ReadingListener {
    int recv_ = 0;

    void on_raw(const uint8_t* data, size_t len); // real body; bound by identity

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace reading_listener_pkg
