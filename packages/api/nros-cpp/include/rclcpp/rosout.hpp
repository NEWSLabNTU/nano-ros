// nros-cpp: `/rosout` — the log bridge rcl gives every node.
// Freestanding C++ — no STL.

/**
 * @file rosout.hpp
 * @ingroup grp_misc
 * @brief `rclcpp::rosout` — publish this image's log records on `/rosout`.
 *
 * Upstream republishes every node's log records on `/rosout` as
 * `rcl_interfaces/msg/Log`; that topic is what `ros2 topic echo /rosout` and
 * `rqt_console` read. Here, as upstream, the runtime does it: in an image
 * built with the `rosout` capability the executor creates one `/rosout`
 * publisher per node and publishes the log queue from every spin. A program
 * only logs through `node.get_logger()`.
 *
 * The runtime's publisher is VOLATILE (KEEP_LAST(queue depth), RELIABLE,
 * 10 s lifespan), where upstream's `rclcpp::RosoutQoS` is TRANSIENT_LOCAL: a
 * tool that subscribes after boot misses the boot records; `ros2 topic echo
 * /rosout` still matches it.
 *
 * `rclcpp::rosout::Publisher` is the explicit spelling from before the runtime
 * owned the publisher. It still works, but adds a second `/rosout` publisher
 * to the node, and is deprecated.
 *
 * **Which records** follow the image's ROS release: on Humble,
 * and with no release named, node loggers only (`node.get_logger()`); on
 * Iron/Jazzy, node loggers and their `get_child` descendants. Free loggers
 * (`rclcpp::get_logger`) never.
 *
 * **Build.** Declare the `rosout` capability (`[system].features =
 * ["rosout"]`, or `NROS_FEATURES`). Without it this header still compiles
 * and links: `enabled()` is false and every call answers
 * `ErrorCode::Unsupported`.
 */

#ifndef NROS_RCLCPP_ROSOUT_HPP
#define NROS_RCLCPP_ROSOUT_HPP

#include <cstddef>
#include <cstdint>

#include "rclcpp/node.hpp"
#include "rclcpp/qos.hpp"
#include "rclcpp/result.hpp"
#include "rclcpp/detail/nros_cpp_ffi.h"

namespace rclcpp {
namespace rosout {

/// `rcl_logging_rosout_enabled()`: true iff this image was built with the
/// `rosout` capability. A build-time answer, as upstream's is.
inline bool enabled() {
    return nros_cpp_rosout_enabled();
}

/// Start queueing log records for `/rosout`. Creating a node already does
/// this; kept for the explicit spelling.
inline Result enable() {
    return Result(nros_cpp_rosout_enable());
}

/// The `/rosout` publisher: topic `/rosout` (absolute — no namespace or remap
/// moves it), type `rcl_interfaces/msg/Log`. Non-copyable, non-movable: the
/// runtime's publisher lives in this object's storage.
class [[deprecated("the executor now creates a /rosout publisher for every node and "
                   "publishes the log queue from every spin; remove this object")]] Publisher {
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

    /// Create it with an explicit profile — `::rclcpp::RosoutQoS()` is upstream's
    /// TRANSIENT_LOCAL KEEP_LAST(1000), for a target that has budgeted for it.
    Result create(::rclcpp::Node& node, const ::rclcpp::QoS& qos) {
        nros_cpp_qos_t ffi = ::rclcpp::detail::qos_to_ffi(qos);
        return create_impl(node, &ffi);
    }

    /// Publish every queued record. Call it from the spin loop; it never
    /// blocks on the queue and never allocates. `out_sent`, if non-null,
    /// receives how many messages reached the transport.
    Result pump(size_t* out_sent = nullptr) {
        if (!initialized_) return Result(::rclcpp::ErrorCode::NotInitialized);
        return Result(nros_cpp_rosout_pump(storage_, out_sent));
    }

    bool is_valid() const { return initialized_; }

  private:
    Result create_impl(::rclcpp::Node& node, const nros_cpp_qos_t* qos) {
        if (initialized_) return Result(::rclcpp::ErrorCode::AlreadyExists);
        const nros_cpp_node_t* h = node.ffi_handle();
        if (h == nullptr) return Result(::rclcpp::ErrorCode::NotInitialized);
        nros_cpp_ret_t ret = nros_cpp_rosout_publisher_create(h, qos, storage_);
        if (ret == 0) initialized_ = true;
        return Result(ret);
    }

    // Same slot shape as `rclcpp::Publisher<M>` — one runtime publisher.
    alignas(8) uint8_t storage_[NROS_PUBLISHER_SIZE + sizeof(void*)];
    bool initialized_;
};

} // namespace rosout
} // namespace rclcpp

#endif // NROS_RCLCPP_ROSOUT_HPP
