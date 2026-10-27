/**
 * @file generic.hpp
 * @ingroup grp_pubsub
 * @brief `rclcpp::GenericPublisher` / `GenericSubscription` — type-erased
 *        pub/sub over CDR bytes, keyed by a type name AND hash (phase-515 P4).
 *
 * rclcpp's `create_generic_publisher(topic, type, qos)` names a type and loads
 * its typesupport at runtime. nano-ros links its types at build time, and the
 * wire key carries the type's RIHS01 hash, which nothing here can derive from
 * a name. So:
 *
 * - rclcpp's name-only spellings are REFUSE-LOUD
 *   (`NROS_RCLCPP_REFUSE_GENERIC_BY_NAME`): a ported call is a compile error
 *   naming the alternative, not "no member named create_generic_publisher";
 * - the hash-taking spellings below are the nano-ros form, the C++ twin of
 *   Rust's `Node::create_generic_publisher(topic, type_name, type_hash)`.
 *   `type` is the wire type name as a generated type spells it in
 *   `M::TYPE_NAME` (`std_msgs::msg::dds_::String_`), and `type_hash` is its
 *   `M::TYPE_HASH`.
 *
 * Bytes in, bytes out: the payload is CDR, never deserialised. That is what a
 * relay or a bridge needs, and it is all a type the image does not link can
 * offer.
 */

#ifndef NROS_RCLCPP_GENERIC_HPP
#define NROS_RCLCPP_GENERIC_HPP

#include <cstddef>
#include <cstdint>
#include <string.h> // memcpy — `<cstring>` isn't in Zephyr's minimal libcpp

#include "rclcpp/detail/entity_name.hpp"
#include "rclcpp/detail/nros_cpp_ffi.h"
#include "rclcpp/inplace_fn.hpp"
#include "rclcpp/logging.hpp"
#include "rclcpp/owned.hpp"
#include "rclcpp/publisher.hpp" // PUBLISHER_TOPIC_NAME_MAX, qos_to_ffi / qos_from_ffi
#include "rclcpp/result.hpp"
#include "rclcpp/size_bound.hpp"   // rclcpp::rx_bound_unknown
#include "rclcpp/subscription.hpp" // nros_cpp_subscription_register_capturing
#include "rclcpp/subscription_handle.hpp"

namespace rclcpp {

class GenericPublisher;

namespace detail {
inline Result create_generic_publisher(const nros_cpp_node_t* node, GenericPublisher& out,
                                       const char* topic, const char* type, const char* type_hash,
                                       const ::rclcpp::QoS& qos);
} // namespace detail

/// A publisher of CDR bytes on a topic whose type is given by name and hash
/// rather than by a linked message type.
///
/// Created by `Node::create_generic_publisher(topic, type, type_hash, qos)`.
/// Same storage and lifetime rules as `Publisher<M>`: movable, not copyable,
/// destroyed with its owner.
class GenericPublisher {
  public:
    /// `rclcpp::GenericPublisher::SharedPtr` — `Owned<GenericPublisher>`, as
    /// `Publisher<M>::SharedPtr` is.
    using SharedPtr = ::rclcpp::Owned<GenericPublisher>;
    /// The same type as `SharedPtr`.
    using ConstSharedPtr = ::rclcpp::Owned<GenericPublisher>;
    /// The same type as `SharedPtr`.
    using UniquePtr = ::rclcpp::Owned<GenericPublisher>;

    GenericPublisher() : storage_(), topic_name_{}, initialized_(false) {}

    /// Publish one CDR-encoded message. `publish_serialized`, as on
    /// `Publisher<M>`: a `(data, len)` pair, not an `rclcpp::SerializedMessage`.
    Result publish_serialized(const uint8_t* data, size_t len) {
        if (!initialized_) return Result(::rclcpp::ErrorCode::NotInitialized);
        return Result(nros_cpp_publish_raw(storage_, data, len));
    }

    const char* get_topic_name() const { return initialized_ ? topic_name_ : ""; }

    ::rclcpp::QoS get_actual_qos() const {
        nros_cpp_qos_t f{};
        if (!initialized_ || nros_cpp_publisher_get_actual_qos(storage_, &f) != 0) {
            return ::rclcpp::detail::qos_all_unknown();
        }
        return ::rclcpp::detail::qos_from_ffi(f);
    }

    bool is_valid() const { return initialized_; }

    ~GenericPublisher() {
        if (initialized_) {
            nros_cpp_publisher_destroy(storage_);
            initialized_ = false;
        }
    }

    GenericPublisher(GenericPublisher&& other) : initialized_(other.initialized_) {
        if (other.initialized_) {
            nros_cpp_publisher_relocate(other.storage_, storage_);
            ::memcpy(topic_name_, other.topic_name_, sizeof(topic_name_));
            other.initialized_ = false;
        }
    }

    GenericPublisher& operator=(GenericPublisher&& other) {
        if (this != &other) {
            if (initialized_) {
                nros_cpp_publisher_destroy(storage_);
                initialized_ = false;
            }
            if (other.initialized_) {
                nros_cpp_publisher_relocate(other.storage_, storage_);
                ::memcpy(topic_name_, other.topic_name_, sizeof(topic_name_));
                initialized_ = true;
                other.initialized_ = false;
            }
        }
        return *this;
    }

  private:
    GenericPublisher(const GenericPublisher&) = delete;
    GenericPublisher& operator=(const GenericPublisher&) = delete;

    friend Result detail::create_generic_publisher(const nros_cpp_node_t*, GenericPublisher&,
                                                   const char*, const char*, const char*,
                                                   const ::rclcpp::QoS&);

    // The same storage `Publisher<M>` carries; see its note on the monitor cell.
    alignas(8) uint8_t storage_[NROS_PUBLISHER_SIZE + sizeof(void*)];
    char topic_name_[::rclcpp::PUBLISHER_TOPIC_NAME_MAX];
    bool initialized_;
};

/// Tag type for a generic subscription's handle. The subscription itself
/// lives in the executor's arena, as every callback-style subscription does;
/// `GenericSubscription::SharedPtr` is the handle to it.
class GenericSubscription {
  public:
    /// `rclcpp::GenericSubscription::SharedPtr` — a handle into the arena,
    /// as `Subscription<M>::SharedPtr` is. `reset()` unregisters.
    using SharedPtr = ::rclcpp::SubscriptionHandle<GenericSubscription>;
    /// The same type as `SharedPtr`.
    using ConstSharedPtr = ::rclcpp::SubscriptionHandle<GenericSubscription>;

    /// The callback's signature: one CDR-encoded message, lent for the call.
    using Callback = ::rclcpp::InplaceFn<void(const uint8_t* data, size_t len)>;
};

namespace detail {

/// Create on `node`; what `Node::create_generic_publisher` calls. Leaves
/// `out` untouched on failure.
inline Result create_generic_publisher(const nros_cpp_node_t* node, GenericPublisher& out,
                                       const char* topic, const char* type, const char* type_hash,
                                       const ::rclcpp::QoS& qos) {
    if (node == nullptr) return Result(::rclcpp::ErrorCode::NotInitialized);
    if (type == nullptr || type_hash == nullptr) {
        return Result(::rclcpp::ErrorCode::InvalidArgument);
    }
    const nros_cpp_ret_t ret = nros_cpp_publisher_create(
        node, topic, type, type_hash, ::rclcpp::detail::qos_to_ffi(qos), out.storage_);
    if (ret == 0) {
        ::rclcpp::detail::assign_entity_name(out.topic_name_, topic);
        out.initialized_ = true;
    }
    return Result(ret);
}

/// Register on `node`; what `Node::create_generic_subscription` calls.
/// The callback is copied into the arena entry.
inline Result register_generic_subscription(const nros_cpp_node_t* node, const char* topic,
                                            const char* type, const char* type_hash,
                                            const ::rclcpp::QoS& qos,
                                            GenericSubscription::Callback& fn,
                                            size_t* out_handle_id) {
    if (node == nullptr) return Result(::rclcpp::ErrorCode::NotInitialized);
    if (type == nullptr || type_hash == nullptr) {
        return Result(::rclcpp::ErrorCode::InvalidArgument);
    }
    nros_cpp_subscription_message_callback_t invoke = [](const uint8_t* data, size_t len,
                                                         void* ctx) {
        // `ctx` is the ARENA's copy of `fn`.
        (*static_cast<const GenericSubscription::Callback*>(ctx))(data, len);
    };
    void (*destroy)(void*) = [](void* ctx) {
        static_cast<GenericSubscription::Callback*>(ctx)->clear();
    };
    // The type is not linked, so its bound is not known here: say so with
    // `rx_bound_unknown`, and the runtime's default receive buffer applies.
    nros_cpp_subscription_options_t opts = nros_cpp_subscription_default_options();
    opts.rx_buffer_hint = static_cast<uint32_t>(::rclcpp::rx_bound_unknown);
    const Result r(nros_cpp_subscription_register_capturing(
        node, topic, type, type_hash, ::rclcpp::detail::qos_to_ffi(qos), invoke,
        reinterpret_cast<const uint8_t*>(&fn), sizeof(GenericSubscription::Callback), destroy,
        out_handle_id, &opts));
    if (r.ok()) {
        // The arena's copy owns the callable now.
        fn.relinquish();
    }
    return r;
}

} // namespace detail

} // namespace rclcpp

#endif // NROS_RCLCPP_GENERIC_HPP
