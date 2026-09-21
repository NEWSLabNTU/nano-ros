// nros-cpp: the poll-style subscriptions
// Freestanding C++ — no exceptions, no STL required

/**
 * @file polling_subscription.hpp
 * @ingroup grp_pubsub
 * @brief The two POLL-side subscribers — `nros::PollSubscription<M>` (consuming
 *        take) and `nros::PollingSubscription<M>` (retained latest value).
 *
 * WHICH ONE DO I WANT
 *
 *   `PollSubscription<M>`   — CONSUMING. Each `take()` yields a sample once and
 *                             then answers `TryAgain`. The caller owns the
 *                             subscriber's storage and drives `spin_once()`
 *                             itself. This is the type that carries `take`,
 *                             `take_serialized`, `take_validated`,
 *                             `take_sequence` and `try_borrow`.
 *   `PollingSubscription<M>`— RETAINED. A `PollSubscription<M>` plus a cached
 *                             newest value, so a caller can read it repeatably
 *                             at a chosen point (issue 0278 — the nano-ros
 *                             analog of ROS 2
 *                             `autoware_utils::InterProcessPollingSubscriber`).
 *
 * Neither dispatches. A subscription with a CALLBACK is `rclcpp::Subscription<M>`
 * in `nros/subscription.hpp`, and the executor arena owns it.
 *
 * WHY THE TAKING API LIVES HERE — phase-456 W2b
 *
 * It used to live on `Subscription<M>`, which served BOTH ownership models
 * behind a `callback_mode_` flag. On the dispatch path the arena owns the
 * subscriber and `storage_` is never filled, so `take()` on a callback-mode
 * object reached
 *
 *     let sub = &mut *(storage as *mut RmwSubscriber)   // subscription.rs:693
 *
 * with 656 value-initialized zero bytes. `nros.hpp` described that as answering
 * `NotInitialized`; it does not — nothing on that path ever checks, because
 * `initialized_` is true. Splitting the types is what makes the call
 * unwritable rather than merely discouraged.
 */

#ifndef NROS_CPP_POLLING_SUBSCRIPTION_HPP
#define NROS_CPP_POLLING_SUBSCRIPTION_HPP

#include <cstdint>
#include <cstddef>
#include <string.h> // memcpy — `<cstring>` isn't in Zephyr's minimal libcpp

#include "nros/node.hpp"
#include "nros/result.hpp"
#include "nros/size_bound.hpp" // nros::rx_buffer_capacity<M> — the receive-buffer size
// RFC-0088 D5 — NROS_CPP_ASSERT_MESSAGE_FORMAT, expanded in the creator below.
#include "nros/serialization_format.hpp"
#include "nros/stream.hpp"
#include "nros/subscription.hpp"
#include "nros/traits.hpp"

// phase-456 W5 — the nested pointer aliases below are `nros::Owned<T>`, which
// is ours and needs no `<memory>`, so the capability probe this header used to
// include is gone with the block it served.
#include "nros/owned.hpp"

#include "nros_cpp_ffi.h"

// phase-427 W7 — `Node` is DEFINED in `rclcpp::` (RFC-0089: that namespace is
// the home). The friend declaration below is qualified, and a qualified friend
// names an existing entity rather than introducing one, so the name has to be
// declared first — and in `rclcpp::`, because an elaborated `class Node;` in
// `nros::` would declare a second, distinct class.
namespace rclcpp {
class Node;
}

namespace nros {

/// Poll-style subscription — caller-owned storage, consuming receive.
///
/// phase-456 W2b: this is the class `Subscription<M>` used to be when it was
/// created through the out-ref `create_subscription(out, topic, qos)`. The name
/// moved because upstream's `rclcpp::Subscription<M>` is owned by the node and
/// dispatched by the executor, which is what OUR `Subscription<M>` now is; an
/// entity a caller declares in its own frame and drains itself has no upstream
/// counterpart, so it is ours and says so.
///
/// Usage:
/// ```cpp
/// nros::PollSubscription<std_msgs::msg::String> sub;
/// NROS_TRY(node.create_subscription(sub, "/chatter"));
/// nros::spin_once(10);
/// uint8_t buf[256];
/// size_t len;
/// if (sub.take_serialized(buf, sizeof(buf), len)) {
///     // process buf[0..len]
/// }
/// ```
template <typename M> class PollSubscription {
  public:
    /// Try to receive a typed message (non-blocking).
    ///
    /// Receives raw CDR data into a stack buffer, then deserializes into `msg`
    /// using the codegen-generated `M::ffi_deserialize()`.
    ///
    /// @param msg  Output message struct (filled on success).
    /// @return Result::success() if a message was received and deserialized;
    ///         ErrorCode::TryAgain if no data is available right now;
    ///         ErrorCode::NotInitialized if the subscription is not initialized;
    ///         ErrorCode::Error if deserialization failed.
    Result take(M& msg) { return take_sized<::nros::rx_buffer_capacity<M>::value>(msg); }

    /// @ref take with the receive buffer sized by the CALLER.
    ///
    /// The escape hatch for a type with no derived bound, and the way to
    /// deliberately override a bounded type's own number. Mirrors how
    /// `bind_subscription_sized` relates to `bind_subscription` (issue 0964).
    ///
    /// @tparam Cap  Stack bytes to receive into. A sample larger than this is
    ///              refused by the backend, so under-sizing DROPS messages.
    template <size_t Cap> Result take_sized(M& msg) {
        if (!initialized_) return Result(::nros::ErrorCode::NotInitialized);
        uint8_t buf[Cap];
        size_t len = 0;
        nros_cpp_ret_t ret =
            nros_cpp_subscription_take_serialized(storage_, buf, sizeof(buf), &len);
        if (ret != 0) return Result(ret);
        if (len == 0) return Result(::nros::ErrorCode::TryAgain);
        if (M::ffi_deserialize(buf, len, &msg) != 0) return Result(::nros::ErrorCode::Error);
        return Result::success();
    }

    /// Issue 0073 — take that ALSO returns the E2E message-integrity status
    /// (CRC + sequence gap/dup) of the received sample. The safety-e2e analog of
    /// @ref take: the backend recomputes + compares the CRC the publisher
    /// attached and tracks the sequence, writing the verdict to @p status.
    ///
    /// Requires the build to enable `safety-e2e` on both ends (the zenoh backend's
    /// own feature, lowered from a declared `[safety]` axis); a binary built
    /// without it cannot link this (the FFI symbol is gated). With it, but against
    /// a publisher built without safety, `status.crc_valid` reports `-1`.
    ///
    /// @param msg     Output message struct (filled on success).
    /// @param status  Receives `{ gap, duplicate, crc_valid }` (crc_valid:
    ///                1=valid, 0=mismatch, -1=no CRC on the wire).
    /// @return Result::success() on a received+deserialized message; TryAgain if
    ///         none available; NotInitialized / Error otherwise.
    Result take_validated(M& msg, nros_cpp_integrity_status_t& status) {
        return take_validated_sized<::nros::rx_buffer_capacity<M>::value>(msg, status);
    }

    /// @ref take_validated with the receive buffer sized by the CALLER.
    /// See @ref take_sized (issue 0964).
    template <size_t Cap> Result take_validated_sized(M& msg, nros_cpp_integrity_status_t& status) {
        if (!initialized_) return Result(::nros::ErrorCode::NotInitialized);
        uint8_t buf[Cap];
        size_t len = 0;
        nros_cpp_ret_t ret =
            nros_cpp_subscription_take_validated(storage_, buf, sizeof(buf), &len, &status);
        if (ret != 0) return Result(ret);
        if (len == 0) return Result(::nros::ErrorCode::TryAgain);
        if (M::ffi_deserialize(buf, len, &msg) != 0) return Result(::nros::ErrorCode::Error);
        return Result::success();
    }

    /// Try to receive raw CDR data (non-blocking).
    ///
    /// Sets `out_len` to the number of bytes received (0 if no data).
    ///
    /// @param buf       Buffer to receive CDR data.
    /// @param capacity  Size of the buffer.
    /// @param out_len   Receives the number of bytes (0 if no data available).
    /// @return Result::success() if data was received; ErrorCode::TryAgain
    ///         if no data is available; ErrorCode::NotInitialized or the
    ///         FFI error code otherwise.
    Result take_serialized(uint8_t* buf, size_t capacity, size_t& out_len) {
        if (!initialized_) {
            out_len = 0;
            return Result(::nros::ErrorCode::NotInitialized);
        }
        nros_cpp_ret_t ret =
            nros_cpp_subscription_take_serialized(storage_, buf, capacity, &out_len);
        if (ret != 0) return Result(ret);
        if (out_len == 0) return Result(::nros::ErrorCode::TryAgain);
        return Result::success();
    }

    /// Try to receive raw CDR data **plus the sample's wire attachment**
    /// (non-blocking) — the C++ poll-side analog of the Rust
    /// `node.subscription(t).generic(..).message_info()` builder
    /// (Phase 189.M3.4b). The attachment carries out-of-band tags such as a
    /// cross-RMW bridge's `bridge_origin`; `out_att_len` is 0 when the sample
    /// carried none.
    ///
    /// @param buf          Buffer to receive CDR payload.
    /// @param capacity     Size of `buf`.
    /// @param out_len      Receives payload length (0 if no data).
    /// @param att          Buffer to receive the attachment.
    /// @param att_capacity Size of `att`.
    /// @param out_att_len  Receives attachment length (0 if none).
    /// @return Result::success() if a sample was received; ErrorCode::TryAgain
    ///         if none is available; NotInitialized / FFI error otherwise.
    Result take_serialized_with_attachment(uint8_t* buf, size_t capacity, size_t& out_len,
                                           uint8_t* att, size_t att_capacity, size_t& out_att_len) {
        if (!initialized_) {
            out_len = 0;
            out_att_len = 0;
            return Result(::nros::ErrorCode::NotInitialized);
        }
        nros_cpp_ret_t ret = nros_cpp_subscription_take_serialized_with_attachment(
            storage_, buf, capacity, &out_len, att, att_capacity, &out_att_len);
        if (ret != 0) return Result(ret);
        if (out_len == 0) return Result(::nros::ErrorCode::TryAgain);
        return Result::success();
    }

    // ====================================================================
    // Phase 124.A.7 — zero-copy receive (borrow / release)
    // ====================================================================

    /// Phase 124.A.7 — read-only view returned by `PollSubscription::borrow`.
    /// RAII: `Drop` releases the view back to the subscriber.
    class View {
      public:
        View() : sub_(nullptr), buf_(nullptr), len_(0), token_(nullptr) {}
        View(View&& o) : sub_(o.sub_), buf_(o.buf_), len_(o.len_), token_(o.token_) {
            o.sub_ = nullptr;
            o.token_ = nullptr;
        }
        View& operator=(View&& o) {
            if (this != &o) {
                release();
                sub_ = o.sub_;
                buf_ = o.buf_;
                len_ = o.len_;
                token_ = o.token_;
                o.sub_ = nullptr;
                o.token_ = nullptr;
            }
            return *this;
        }
        View(const View&) = delete;
        View& operator=(const View&) = delete;
        ~View() { release(); }

        const uint8_t* data() const { return buf_; }
        size_t size() const { return len_; }
        bool empty() const { return token_ == nullptr; }

        /// Internal constructor — callers use `PollSubscription::try_borrow()`.
        View(void* sub, const uint8_t* buf, size_t len, void* token)
            : sub_(sub), buf_(buf), len_(len), token_(token) {}

      private:
        void release() {
            if (token_ && sub_) {
                nros_cpp_subscription_release(sub_, token_);
                token_ = nullptr;
            }
        }

        void* sub_;
        const uint8_t* buf_;
        size_t len_;
        void* token_;
    };

    /// Phase 124.A.7 — try to borrow the next message in place. Returns
    /// `View` with data when a message is ready, empty `View` when not.
    /// On error returns `ResultOf::error`.
    ::nros::ResultOf<View> try_borrow() {
        if (!initialized_)
            return ::nros::ResultOf<View>::error(Result(::nros::ErrorCode::NotInitialized));
        const uint8_t* buf = nullptr;
        size_t len = 0;
        void* token = nullptr;
        int32_t rc = nros_cpp_subscription_borrow(storage_, &buf, &len, &token);
        if (rc < 0) return ::nros::ResultOf<View>::error(Result(rc));
        if (rc == 0) return ::nros::ResultOf<View>::ok(View{});
        return ::nros::ResultOf<View>::ok(View{storage_, buf, len, token});
    }

    /// Phase 124.D.1 — burst-take.
    ///
    /// Drain up to `max_msgs` queued samples in a single call. The
    /// i-th delivered sample lives at `buf + i * per_msg_cap` with
    /// length `out_lens[i]`. Writes the count to `out_count`. Returns
    /// `Result::success()` on success (count may be 0), the matching
    /// FFI error otherwise.
    ///
    /// Backends without a native batch take fall back to a
    /// `take_serialized` loop — same shape, same observable result;
    /// the batched API just lets sensor loops commit to one call
    /// shape regardless of backend support.
    Result take_sequence(uint8_t* buf, size_t per_msg_cap, size_t max_msgs, size_t* out_lens,
                         size_t& out_count) {
        if (!initialized_) {
            out_count = 0;
            return Result(::nros::ErrorCode::NotInitialized);
        }
        nros_cpp_ret_t ret = nros_cpp_subscription_take_sequence(storage_, buf, per_msg_cap,
                                                                 max_msgs, out_lens, &out_count);
        if (ret != 0) return Result(ret);
        return Result::success();
    }

    // ====================================================================
    // DEPRECATED spellings — phase-379 W6 decision 1 (2026-09-03)
    // ====================================================================
    //
    // `take` -> `take`, `_raw` -> `_serialized`. rcl (`rcl_take`,
    // `rcl_take_serialized_message`), rclcpp (`Subscription::take`,
    // `take_serialized`) and our OWN RMW vtable one layer down (`take`,
    // `take_sequence`) all spell the non-blocking consuming receive that way.
    // Only this user-facing layer said `take` — Rust-channel vocabulary
    // that reads as a different contract to a ROS 2 user, when both forms are
    // non-blocking and both report emptiness without failing.
    //
    // Header-only forwarders, so there is no ABI cost and the new name is the
    // only one a reader of this class sees first. Scheduled for removal.

    /// @deprecated Use `take(M&)`.
    [[deprecated("PollSubscription::try_recv is deprecated; use PollSubscription::take")]] Result
    try_recv(M& msg) {
        return take(msg);
    }

    /// @deprecated Use `take_sized<Cap>(M&)`.
    ///
    /// phase-379 W6 — this pair arrived on main (issue 0964) between the W6
    /// decision and its execution, so it gets the same forwarder treatment as
    /// the five spellings the decision named. Retired with them in W7 step 4.
    template <size_t Cap>
    [[deprecated("PollSubscription::try_recv_sized is deprecated; use "
                 "PollSubscription::take_sized")]] Result
    try_recv_sized(M& msg) {
        return take_sized<Cap>(msg);
    }

    /// @deprecated Use `take_validated_sized<Cap>(M&, nros_cpp_integrity_status_t&)`.
    template <size_t Cap>
    [[deprecated("PollSubscription::try_recv_validated_sized is deprecated; use "
                 "PollSubscription::take_validated_sized")]] Result
    try_recv_validated_sized(M& msg, nros_cpp_integrity_status_t& status) {
        return take_validated_sized<Cap>(msg, status);
    }

    /// @deprecated Use `take_validated(M&, nros_cpp_integrity_status_t&)`.
    [[deprecated("PollSubscription::try_recv_validated is deprecated; use "
                 "PollSubscription::take_validated")]] Result
    try_recv_validated(M& msg, nros_cpp_integrity_status_t& status) {
        return take_validated(msg, status);
    }

    /// @deprecated Use `take_serialized(uint8_t*, size_t, size_t&)`.
    [[deprecated("PollSubscription::try_recv_raw is deprecated; use "
                 "PollSubscription::take_serialized")]] Result
    try_recv_raw(uint8_t* buf, size_t capacity, size_t& out_len) {
        return take_serialized(buf, capacity, out_len);
    }

    /// @deprecated Use `take_serialized_with_attachment(...)`.
    [[deprecated("PollSubscription::try_recv_raw_with_attachment is deprecated; use "
                 "PollSubscription::take_serialized_with_attachment")]] Result
    try_recv_raw_with_attachment(uint8_t* buf, size_t capacity, size_t& out_len, uint8_t* att,
                                 size_t att_capacity, size_t& out_att_len) {
        return take_serialized_with_attachment(buf, capacity, out_len, att, att_capacity,
                                               out_att_len);
    }

    /// @deprecated Use `take_sequence(...)`.
    [[deprecated("PollSubscription::try_recv_sequence is deprecated; use "
                 "PollSubscription::take_sequence")]] Result
    try_recv_sequence(uint8_t* buf, size_t per_msg_cap, size_t max_msgs, size_t* out_lens,
                      size_t& out_count) {
        return take_sequence(buf, per_msg_cap, max_msgs, out_lens, out_count);
    }

    /// Get the topic name.
    const char* get_topic_name() const { return initialized_ ? topic_name_ : ""; }

    /// Get a reference to the subscription's message stream.
    ///
    /// Use for blocking reception with executor spin:
    /// ```cpp
    /// M msg;
    /// NROS_TRY(sub.stream().wait_next(executor.handle(), 1000, msg));
    /// ```
    ::nros::Stream<M>& stream() {
        if (initialized_ && !stream_.is_valid()) {
            stream_.bind(storage_, &nros_cpp_subscription_take_serialized);
        }
        return stream_;
    }

    const ::nros::Stream<M>& stream() const { return stream_; }

    /// Check if the subscription is initialized and valid.
    bool is_valid() const { return initialized_; }

    /// Destructor — releases the subscriber this object owns.
    ///
    /// Unconditional since phase-456 W2b: every `PollSubscription<M>` owns an
    /// `RmwSubscriber` in `storage_`. The `!callback_mode_` guard this replaced
    /// existed because one class served two owners; the dispatch entity is
    /// `rclcpp::Subscription<M>` now and has no storage to free.
    ~PollSubscription() {
        if (initialized_) {
            nros_cpp_subscription_destroy(storage_);
        }
        initialized_ = false;
    }

    // Move semantics (non-copyable). Relocation goes through the
    // `nros_cpp_subscription_relocate` runtime call (Phase 84.C1);
    // the `stream_` is rebound to the new storage afterwards.
    //
    // Safe for every object of this type, which is new in phase-456 W2b: the
    // old class also held callback-mode entities, and moving one of THOSE left
    // the arena dispatching into a stale `this`. That hazard moved with its
    // subject.
    PollSubscription(PollSubscription&& other) : initialized_(other.initialized_) {
        if (other.initialized_) {
            nros_cpp_subscription_relocate(other.storage_, storage_);
            ::memcpy(topic_name_, other.topic_name_, sizeof(topic_name_));
            stream_.bind(storage_, &nros_cpp_subscription_take_serialized);
        }
        other.initialized_ = false;
        other.stream_ = ::nros::Stream<M>();
    }

    PollSubscription& operator=(PollSubscription&& other) {
        if (this != &other) {
            if (initialized_) {
                nros_cpp_subscription_destroy(storage_);
                stream_ = ::nros::Stream<M>();
            }
            initialized_ = other.initialized_;
            if (other.initialized_) {
                nros_cpp_subscription_relocate(other.storage_, storage_);
                ::memcpy(topic_name_, other.topic_name_, sizeof(topic_name_));
                stream_.bind(storage_, &nros_cpp_subscription_take_serialized);
            }
            other.initialized_ = false;
            other.stream_ = ::nros::Stream<M>();
        }
        return *this;
    }

    /// Default constructor — creates an uninitialized subscription.
    /// Use `Node::create_subscription()` to initialize.
    PollSubscription() : storage_(), topic_name_{}, initialized_(false), stream_() {}

    // ====================================================================
    // Phase 108 — status events
    // ====================================================================
    //
    // These take `storage_`, so they were only ever answerable on this half of
    // the old class: on a callback-mode object `storage_` is 656 zero bytes the
    // arena never filled. They live here now for that reason, and the dispatch
    // entity offers no such setter rather than one that reinterprets zeros.

    /// Register a callback for liveliness-changed events.
    ///
    /// Returns `Result(ErrorCode::Unsupported)` until the active
    /// backend wires up liveliness detection.
    Result on_liveliness_changed(nros_cpp_liveliness_changed_cb_t cb,
                                 void* user_context = nullptr) {
        if (!initialized_) return Result(::nros::ErrorCode::NotInitialized);
        return Result(nros_cpp_subscription_set_liveliness_changed(storage_, cb, user_context));
    }

    /// Register a callback for requested-deadline-missed events.
    Result on_requested_deadline_missed(uint32_t deadline_ms, nros_cpp_subscriber_count_cb_t cb,
                                        void* user_context = nullptr) {
        if (!initialized_) return Result(::nros::ErrorCode::NotInitialized);
        return Result(nros_cpp_subscription_set_requested_deadline_missed(storage_, deadline_ms, cb,
                                                                          user_context));
    }

    /// Register a callback for message-lost events.
    Result on_message_lost(nros_cpp_subscriber_count_cb_t cb, void* user_context = nullptr) {
        if (!initialized_) return Result(::nros::ErrorCode::NotInitialized);
        return Result(nros_cpp_subscription_set_message_lost(storage_, cb, user_context));
    }

  private:
    PollSubscription(const PollSubscription&) = delete;
    PollSubscription& operator=(const PollSubscription&) = delete;

    friend class ::rclcpp::Node;

    alignas(8) uint8_t storage_[NROS_SUBSCRIBER_SIZE];
    char topic_name_[::nros::SUBSCRIPTION_TOPIC_NAME_MAX];
    bool initialized_;
    ::nros::Stream<M> stream_;
};

/// Latest-value polling subscription.
///
/// Usage:
/// ```cpp
/// nros::PollingSubscription<std_msgs::msg::Int32> sub;
/// NROS_TRY(node.create_polling_subscription(sub, "/count"));
/// // ... later, in a timer tick or the main loop:
/// nros::spin_once(executor, 0);           // pump the transport
/// if (const auto* v = sub.take_data()) {  // newest known value, or nullptr
///     use(v->data);
/// }
/// ```
template <typename M> class PollingSubscription {
  public:
    /// `PollingSubscription<M>::SharedPtr` — phase-456 W5.
    ///
    /// The nested-pointer spelling rclcpp uses for every entity type, carried
    /// here so a member declaration reads the same as its
    /// `Subscription<M>::SharedPtr` sibling. There is no upstream
    /// `PollingSubscription`; the analog is `autoware_utils::
    /// InterProcessPollingSubscriber` (issue 0278).
    ///
    /// `nros::Owned<PollingSubscription<M>>`, and the reason is the ownership
    /// rather than a preference: this entity's subscriber lives in the CALLER's
    /// storage, so there is no arena slot to hand back a handle to, and the
    /// object itself is what a holder must hold. `ConstSharedPtr` and
    /// `UniquePtr` are the same type for the reasons `owned.hpp` gives.
    ///
    /// It also exists on every target now, which the `std::shared_ptr` it
    /// replaced did not — a freestanding leaf that declares one of these was
    /// the case the old gate silently removed the alias from.
    using SharedPtr = ::nros::Owned<PollingSubscription<M>>;
    /// `PollingSubscription<M>::ConstSharedPtr` — see `SharedPtr`.
    using ConstSharedPtr = ::nros::Owned<PollingSubscription<M>>;
    /// `PollingSubscription<M>::UniquePtr` — see `SharedPtr`.
    using UniquePtr = ::nros::Owned<PollingSubscription<M>>;

    PollingSubscription() : sub_(), latest_(), has_ever_(false) {}

    PollingSubscription(const PollingSubscription&) = delete;
    PollingSubscription& operator=(const PollingSubscription&) = delete;

    /// True once the underlying subscription is created.
    bool is_valid() const { return sub_.is_valid(); }

    /// True once at least one sample has ever been received.
    bool has_data() const { return has_ever_; }

    /// Drain to the newest pending sample, then return a pointer to the
    /// retained latest value — repeatably, whether or not a new sample arrived
    /// this call. Returns `nullptr` only if NOTHING has ever been received
    /// (mirrors `InterProcessPollingSubscriber::takeData`).
    const M* take_data() {
        drain();
        return has_ever_ ? &latest_ : nullptr;
    }

    /// Drain to the newest pending sample; return a pointer to it ONLY if a new
    /// sample actually arrived this call, else `nullptr` (mirrors
    /// `takeNewData` — use when "did it change?" matters).
    const M* take_new_data() { return drain() ? &latest_ : nullptr; }

    // phase-379 W6 — `take(M&)` was REMOVED here, deliberately, and the name is
    // now reserved for rclcpp's meaning.
    //
    // It drained to the newest sample and returned `true` if a value had EVER
    // been received, cached or new. `rclcpp::Subscription::take` is CONSUMING:
    // "true if data was taken and is valid". So the idiomatic drain loop
    //
    //     while (sub.take(msg)) { process(msg); }
    //
    // terminated under rclcpp and spun forever on one stale sample here --
    // same name, same signature, opposite contract, no compile error.
    //
    // It was a convenience duplicating `take_data()`, with zero real callers.
    // `take_data()` (retained latest) and `take_new_data()` (only if new) are
    // the faithful `autoware_utils` mirrors (issue 0278) and are unaffected;
    // use `take_data()` for what this did.

    /// Direct read of the cached latest without draining (no transport poll).
    /// `nullptr` until the first value arrives. Pair with an explicit
    /// `take_data()`/`take_new_data()` elsewhere if you only want to poll once.
    const M* peek() const { return has_ever_ ? &latest_ : nullptr; }

  private:
    friend class ::rclcpp::Node;

    /// Consume every pending sample, keeping the newest in `latest_`. Returns
    /// `true` iff at least one new sample was taken this call. `take`
    /// writes `latest_` only on success, so a trailing `TryAgain` leaves the
    /// previous latest intact.
    bool drain() {
        bool got = false;
        while (sub_.take(latest_).ok()) {
            has_ever_ = true;
            got = true;
        }
        return got;
    }

    PollSubscription<M> sub_;
    M latest_;
    bool has_ever_;
};

} // namespace nros

#include "nros/node.hpp"

namespace nros {} // namespace nros

namespace rclcpp {

/// phase-456 W2b — the POLL creator, moved here with the type it creates.
template <typename M>
Result Node::create_subscription(::nros::PollSubscription<M>& out, const char* topic,
                                 const ::nros::QoS& qos) {
    // RFC-0088 D5 — one image, one backend, one encoding. Compile-time, so a
    // message the linked backend cannot encode never reaches the wire.
    NROS_CPP_ASSERT_MESSAGE_FORMAT(M);
    if (!initialized_) return Result(::nros::ErrorCode::NotInitialized);
    nros_cpp_qos_t ffi_qos = ::nros::detail::qos_to_ffi(qos);
    nros_cpp_ret_t ret = nros_cpp_subscription_create(&handle_, topic, M::TYPE_NAME, M::TYPE_HASH,
                                                      ffi_qos, out.storage_);
    if (ret == 0) {
        // Phase 87.6: topic name lives C++-side now.
        size_t topic_len = 0;
        while (topic[topic_len] != '\0' && topic_len + 1 < sizeof(out.topic_name_)) {
            out.topic_name_[topic_len] = topic[topic_len];
            ++topic_len;
        }
        out.topic_name_[topic_len] = '\0';
        out.initialized_ = true;
    }
    return Result(ret);
}

/// Phase 189.M3.1 — named-options overload for the poll creator.
///
/// Delegates to the qos-only create. `options` carries the non-QoS axes, and
/// NEITHER of them reaches a poll subscription:
///
///  * `sched_context` — an executor `HandleId` is what
///    `nros_cpp_bind_handle_to_sched_context` binds, and a poll subscription
///    registers no arena entry, so there is no slot to bind. Until phase-456
///    W2b this read as a runtime guard (`out.has_sched_handle()`, which was
///    `sched_handle_id_ != SIZE_MAX` on a field the poll path never assigned),
///    i.e. a branch that could not be taken; splitting the types makes it
///    structural — the field does not exist on this type.
///  * `message_info` — reserved (M3.4); ignored today.
///
/// The overload stays because `SubscriptionOptions` is how a caller states both
/// axes and the QoS together, and because removing it would break call sites
/// that pass a default-constructed one.
template <typename M>
Result Node::create_subscription(::nros::PollSubscription<M>& out, const char* topic,
                                 const ::nros::QoS& qos,
                                 const ::nros::SubscriptionOptions& options) {
    (void)options;
    return create_subscription<M>(out, topic, qos);
}

template <typename M>
Result Node::create_polling_subscription(::nros::PollingSubscription<M>& out, const char* topic,
                                         const ::nros::QoS& qos) {
    // Reuse the existing poll-mode subscription factory to own storage/init/
    // destroy; the wrapper only adds the retained-latest cache (issue 0278).
    return create_subscription(out.sub_, topic, qos);
}
} // namespace rclcpp

namespace nros {

/// Phase 123.B.4 — value-returning subscription factory. Pairs
/// with `create_publisher` so the full pub/sub create dance is
/// expressible as a chain of `auto`-typed factories.
///
/// phase-456 W2b — returns the POLL type, which is what it always created: it
/// calls the out-ref `create_subscription(out, topic, qos)`, and a value a
/// caller holds by move is a caller-owned entity by definition. A dispatch
/// subscription cannot be handed back this way at all; `rclcpp::Node::
/// create_subscription(topic, qos, callback)` returns its two-word handle.
template <typename M>
inline ResultOf<PollSubscription<M>> create_subscription(::rclcpp::Node& node, const char* topic,
                                                         const QoS& qos = QoS::default_profile()) {
    PollSubscription<M> s;
    Result r = node.create_subscription<M>(s, topic, qos);
    if (!r.ok()) return ResultOf<PollSubscription<M>>::error(r);
    return ResultOf<PollSubscription<M>>::ok(::nros::tr::forward_rvalue(s));
}

} // namespace nros

#endif // NROS_CPP_POLLING_SUBSCRIPTION_HPP
