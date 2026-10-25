// nros-cpp: rclcpp::MessageInfo — a received sample's metadata
// Freestanding C++ — no exceptions, no STL required

/**
 * @file message_info.hpp
 * @ingroup grp_pubsub
 * @brief `rclcpp::MessageInfo` (issue 2036): what a
 *        `(const M&, const rclcpp::MessageInfo&)` subscription callback is
 *        handed with each message.
 */

#ifndef NROS_RCLCPP_MESSAGE_INFO_HPP
#define NROS_RCLCPP_MESSAGE_INFO_HPP

#include <cstdint>

namespace rclcpp {

/// Upstream's `rmw_gid_t`: which publisher sent a sample.
///
/// `data` is upstream's 24 bytes (`RMW_GID_STORAGE_SIZE`). How many of them
/// mean anything is the backend's: zenoh and Cyclone fill 16 and zero-pad the
/// rest, XRCE and uORB report none (all zero). Compare whole arrays.
/// `implementation_identifier` is null: the image has one RMW per session,
/// and the identifier would name it, not the sender.
struct RmwGid {
    const char* implementation_identifier = nullptr;
    uint8_t data[24] = {};
};

/// Upstream's `rmw_message_info_t`, field for field — what
/// `MessageInfo::get_rmw_message_info()` returns. Under its own name, because
/// the C++ surface does not include the RMW ABI headers that define
/// `rmw_message_info_t`; member access reads the same as upstream's.
///
/// A backend that reports no metadata (XRCE, uORB) leaves every field zero:
/// zero is "not reported", never "published at time zero".
struct RmwMessageInfo {
    /// Publisher's clock at publication, ns.
    int64_t source_timestamp = 0;
    /// Subscriber's clock at reception, ns.
    int64_t received_timestamp = 0;
    /// The publisher's sequence number for this sample.
    uint64_t publication_sequence_number = 0;
    /// This subscription's reception count.
    uint64_t reception_sequence_number = 0;
    /// Which publisher sent it.
    RmwGid publisher_gid;
    /// The sample never left the process. Not reported by any in-tree
    /// backend: always `false`.
    bool from_intra_process = false;
};

/// `rclcpp::MessageInfo` — issue 2036. The metadata a
/// `(const M&, const rclcpp::MessageInfo&)` subscription callback receives,
/// filled from the backend's per-sample report (Rust `nros::MessageInfo`:
/// both timestamps, both sequence numbers, the publisher GID).
class MessageInfo {
  public:
    MessageInfo() = default;
    explicit MessageInfo(const RmwMessageInfo& info) : info_(info) {}

    /// Upstream's accessor, both constnesses.
    const RmwMessageInfo& get_rmw_message_info() const { return info_; }
    RmwMessageInfo& get_rmw_message_info() { return info_; }

  private:
    RmwMessageInfo info_;
};

} // namespace rclcpp

#endif // NROS_RCLCPP_MESSAGE_INFO_HPP
