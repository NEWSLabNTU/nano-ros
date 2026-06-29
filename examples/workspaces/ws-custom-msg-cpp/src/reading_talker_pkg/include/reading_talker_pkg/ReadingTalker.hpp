#pragma once

#include <cstddef>
#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>

namespace reading_talker_pkg {

/// Minimal type tag for the workspace-local `custom_msgs/Reading`. Raw-CDR
/// (RFC-0043): `Publisher<M>` needs only `M::TYPE_NAME` / `M::TYPE_HASH` to
/// register the entity; the payload is hand-encoded via `publish_raw`, so no
/// generated header is consumed and `publish()` / `ffi_publish` are never
/// instantiated. Mirrors the metadata the codegen emits for a real message type,
/// matching the committed C demo's raw type-name string exactly.
struct ReadingTag {
    static constexpr const char* TYPE_NAME = "custom_msgs::msg::dds_::Reading_";
    static constexpr const char* TYPE_HASH = "";
};

/// ReadingTalker — typed component (RFC-0043), the C++ projection of
/// ws-custom-msg-c's ReadingTalker. `configure` creates a raw publisher on
/// `/reading` and binds `on_tick` as a 1 Hz timer that publishes a CDR-encoded
/// `Reading` whose `sequence` ramps every tick.
class ReadingTalker {
    ::nros::Publisher<ReadingTag> pub_;
    ::nros::Timer timer_;
    int count_ = 0;

    void on_tick(); // real body; bound via &ReadingTalker::on_tick

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace reading_talker_pkg
