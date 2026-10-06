// nros-cpp: `/rosout` — the log bridge rcl gives every node (issue 1589).
// Freestanding C++ — no STL.

/**
 * @file rosout.hpp
 * @ingroup grp_misc
 * @brief `nros::rosout` — publish this image's log records on `/rosout`.
 *
 * Upstream republishes every node's log records on `/rosout` as
 * `rcl_interfaces/msg/Log`; that topic is what `ros2 topic echo /rosout` and
 * `rqt_console` read. Here it is three steps, and the program takes them:
 *
 * @code
 * nros::rosout::Publisher rosout;
 * NROS_TRY(rosout.create(node));          // bounded QoS; or create(node, nros::RosoutQoS())
 * NROS_TRY(nros::rosout::enable());       // AFTER the publisher
 * for (;;) {
 *     executor.spin_some(10ms);
 *     rosout.pump();
 * }
 * @endcode
 *
 * **Why `rclcpp::NodeOptions::enable_rosout(true)` still refuses.** Upstream's
 * flag turns on a publisher the runtime creates for you. A publisher is an
 * ENTITY here — it counts against the image's pools and sizing descriptor — so
 * the runtime never conjures one below the declaration (issue 1341's shape).
 * This class is the explicit spelling; the Rust bridge (`nros::rosout`) and
 * the C one (`<nros/rosout.h>`) have the same three steps.
 *
 * **Which records** follow the image's ROS release (RFC-0102 D4): on Humble,
 * and with no release named, node loggers only (`node.get_logger()`); on
 * Iron/Jazzy, node loggers and their `get_child` descendants. Free loggers
 * (`rclcpp::get_logger`) never.
 *
 * **Build.** Declare the `rosout` capability (`[system].features =
 * ["rosout"]`, or `NANO_ROS_FEATURES`). Without it this header still compiles
 * and links: `enabled()` is false and every call answers
 * `ErrorCode::Unsupported`.
 */

#ifndef NROS_CPP_ROSOUT_HPP
#define NROS_CPP_ROSOUT_HPP

#include <cstddef>
#include <cstdint>

#include "nros/node.hpp"
#include "nros/qos.hpp"
#include "nros/result.hpp"
#include "nros_cpp_ffi.h"

namespace nros {
namespace rosout {

/// `rcl_logging_rosout_enabled()`: true iff this image was built with the
/// `rosout` capability. A build-time answer, as upstream's is.
inline bool enabled() {
    return nros_cpp_rosout_enabled();
}

/// Start queueing log records for `/rosout`. Call it after
/// `Publisher::create`: records queued with nowhere to drain to are only
/// counted as dropped.
inline Result enable() {
    return Result(nros_cpp_rosout_enable());
}

/// The `/rosout` publisher: topic `/rosout` (absolute — no namespace or remap
/// moves it), type `rcl_interfaces/msg/Log`. Non-copyable, non-movable: the
/// runtime's publisher lives in this object's storage.
class Publisher {
  public:
    Publisher() : storage_(), initialized_(false) {}
    Publisher(const Publisher&) = delete;
    Publisher& operator=(const Publisher&) = delete;

    ~Publisher() {
        if (initialized_) {
            nros_cpp_publisher_destroy(storage_);
            initialized_ = false;
        }
    }

    /// Create it with the bounded profile — KEEP_LAST(queue depth), RELIABLE,
    /// VOLATILE, 10 s lifespan — which needs no transient-local slot. A tool
    /// that subscribes after boot misses the boot records; `ros2 topic echo
    /// /rosout` still matches it.
    Result create(::rclcpp::Node& node) { return create_impl(node, nullptr); }

    /// Create it with an explicit profile — `::nros::RosoutQoS()` is upstream's
    /// TRANSIENT_LOCAL KEEP_LAST(1000), for a target that has budgeted for it.
    Result create(::rclcpp::Node& node, const ::nros::QoS& qos) {
        nros_cpp_qos_t ffi = ::nros::detail::qos_to_ffi(qos);
        return create_impl(node, &ffi);
    }

    /// Publish every queued record. Call it from the spin loop; it never
    /// blocks on the queue and never allocates. `out_sent`, if non-null,
    /// receives how many messages reached the transport.
    Result pump(size_t* out_sent = nullptr) {
        if (!initialized_) return Result(::nros::ErrorCode::NotInitialized);
        return Result(nros_cpp_rosout_pump(storage_, out_sent));
    }

    bool is_valid() const { return initialized_; }

  private:
    Result create_impl(::rclcpp::Node& node, const nros_cpp_qos_t* qos) {
        if (initialized_) return Result(::nros::ErrorCode::AlreadyExists);
        const nros_cpp_node_t* h = node.ffi_handle();
        if (h == nullptr) return Result(::nros::ErrorCode::NotInitialized);
        nros_cpp_ret_t ret = nros_cpp_rosout_publisher_create(h, qos, storage_);
        if (ret == 0) initialized_ = true;
        return Result(ret);
    }

    // Same slot shape as `rclcpp::Publisher<M>` — one runtime publisher.
    alignas(8) uint8_t storage_[NROS_PUBLISHER_SIZE + sizeof(void*)];
    bool initialized_;
};

} // namespace rosout
} // namespace nros

#endif // NROS_CPP_ROSOUT_HPP
