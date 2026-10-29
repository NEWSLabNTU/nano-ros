// nros-cpp: Subscription class
// Freestanding C++ — no exceptions, no STL required

/**
 * @file subscription.hpp
 * @ingroup grp_pubsub
 * @brief `rclcpp::Subscription<M>` — the typed DISPATCH subscriber: the arena
 *        owns it and the executor calls your handler.
 *
 * The taking subscriber is `rclcpp::PollSubscription<M>` in
 * `rclcpp/polling_subscription.hpp` (phase-456 W2b).
 */

#ifndef NROS_RCLCPP_SUBSCRIPTION_HPP
#define NROS_RCLCPP_SUBSCRIPTION_HPP

#include <cstdint>
#include <cstddef>
#include <string.h> // memcpy — `<cstring>` isn't in Zephyr's minimal libcpp

#include "rclcpp/detail/traits.hpp"
#include "rclcpp/detail/config.hpp"
#include "rclcpp/detail/entity_name.hpp" // phase-444 — the one entity-name copy
#include "rclcpp/executor.hpp"           // phase-444 — the graph counter these forward to
#include "rclcpp/result.hpp"
#include "rclcpp/subscription_handle.hpp"
#include "rclcpp/inplace_fn.hpp"
#include "rclcpp/size_bound.hpp" // rclcpp::rx_buffer_capacity<M> — the receive-buffer size
// RFC-0088 D5 — NROS_CPP_ASSERT_MESSAGE_FORMAT, expanded in the creators below.
#include "rclcpp/detail/serialization_format.hpp"
#include "rclcpp/stream.hpp"

// phase-417 W1.a — `<memory>` for the nested pointer aliases below.
// `NROS_CPP_HAS_SHARED_PTR` and the other five capability macros have ONE
// definition site, and the measured reason the predicate needs both probes
// (issues 0112, 1187, 1240) is stated there.
#include "rclcpp/detail/std_detect.hpp"

#include "rclcpp/detail/nros_cpp_ffi.h"

// issue 1437 — `get_actual_qos()` returns a `rclcpp::QoS` BY VALUE from an
// inline body, so the complete type must be here, not only by the time
// `rclcpp/node.hpp` is pulled in below.
//
// AFTER `nros_cpp_ffi.h`, never before: `qos.hpp` defines the four
// `nros_cpp_qos_*_t` enums ITSELF under `#ifndef NROS_CPP_FFI_H`, so
// reaching it first makes the cbindgen header a REDEFINITION of all four.
#include "rclcpp/qos.hpp"
#include "rclcpp/message_info.hpp" // issue 2036 — the two-argument callback shapes

// Phase 189.M3.x — `nros_cpp_subscription_register` is excluded from cbindgen
// (its Rust signature uses `RawSubscriptionCallback`, an external-crate type
// alias cbindgen names without defining). Declare it locally with a plain
// function-pointer typedef matching the ABI (`void(data, len, ctx)`), mirroring
// the service.hpp callback-register treatment.
extern "C" {
typedef void (*nros_cpp_subscription_message_callback_t)(const uint8_t* data, size_t len,
                                                         void* ctx);

// phase-402: the trailing `sched_context` / `callback_group` arguments moved
// into `nros_cpp_subscription_options_t` (defined by the cbindgen output above);
// a NULL `options` is all-defaults, i.e. the pre-phase-402 behaviour.
nros_cpp_ret_t nros_cpp_subscription_register(const nros_cpp_node_t* node, const char* topic,
                                              const char* type_name, const char* type_hash,
                                              nros_cpp_qos_t qos,
                                              nros_cpp_subscription_message_callback_t callback,
                                              void* context, size_t* out_handle_id,
                                              const nros_cpp_subscription_options_t* options);

/// phase-456 W1 — the same registration, with the callback's CAPTURE held by
/// the ARENA rather than by the caller.
///
/// The difference is the lifetime, not the dispatch. `..._register` above
/// passes `context` through untouched, so the caller must keep whatever it
/// points at alive AND unmoved for as long as the executor runs — which is why
/// a callback-style entity was immovable after registration, and why the
/// returning `create_subscription` had to heap-allocate a cell to give a
/// capturing lambda a stable address.
///
/// Here the first `capture_len` bytes at `capture` are COPIED into the arena
/// entry and the dispatch context becomes the entry's own copy. Nothing of the
/// caller's is referenced after the call returns, so the caller may keep
/// nothing at all — which is what lets the C++ object become a handle.
///
/// `capture_len` is a LENGTH, not a budget — phase-456 W8. W1 copied the capture
/// into a fixed `[u8; CALLBACK_CAPTURE_BYTES]` and refused anything longer,
/// where that constant had to equal `NROS_CPP_CALLBACK_CAPACITY` by
/// construction — so the refusal was reachable only when the two languages had
/// drifted apart about a number. The runtime now allocates exactly
/// `capture_len` bytes from the arena, and the only bound left is the arena
/// every other entry already shares.
nros_cpp_ret_t nros_cpp_subscription_register_capturing(
    const nros_cpp_node_t* node, const char* topic, const char* type_name, const char* type_hash,
    nros_cpp_qos_t qos, nros_cpp_subscription_message_callback_t callback, const uint8_t* capture,
    size_t capture_len, void (*capture_drop)(void* capture), size_t* out_handle_id,
    const nros_cpp_subscription_options_t* options);

// Phase 189.M3.4 — callback-style register that also delivers the sample's wire
// attachment (5-arg trampoline). Same cbindgen-exclusion reason as above.
typedef void (*nros_cpp_subscription_message_info_callback_t)(const uint8_t* data, size_t len,
                                                              const uint8_t* attachment,
                                                              size_t attachment_len, void* ctx);

nros_cpp_ret_t nros_cpp_subscription_register_with_info(
    const nros_cpp_node_t* node, const char* topic, const char* type_name, const char* type_hash,
    nros_cpp_qos_t qos, nros_cpp_subscription_message_info_callback_t callback, void* context,
    size_t* out_handle_id, const nros_cpp_subscription_options_t* options);
// Phase 269 W3 — callback-style subscription that delivers the sample's E2E
// integrity status alongside the CDR bytes. Same cbindgen-exclusion reason as
// above (takes `RawSubscriptionSafetyCallback`, an external nros-node alias
// gated on the `safety-e2e` feature). The 6-arg trampoline unpacks the three
// integrity scalars; `subscription.hpp` repacks them into
// `nros_cpp_integrity_status_t` for the typed user handler. Gated on the
// `NROS_SAFETY_E2E` build feature (lowered from `[system].features =
// ["safety"]` via `NanoRosCapabilities.cmake`).
#if defined(NROS_SAFETY_E2E)
typedef void (*nros_cpp_subscription_validated_callback_t)(const uint8_t* data, size_t len,
                                                           int64_t gap, bool duplicate,
                                                           int8_t crc_valid, void* ctx);

nros_cpp_ret_t nros_cpp_subscription_register_validated(
    const nros_cpp_node_t* node, const char* topic, const char* type_name, const char* type_hash,
    nros_cpp_qos_t qos, nros_cpp_subscription_validated_callback_t callback, void* context,
    size_t* out_handle_id, const nros_cpp_subscription_options_t* options);
#endif // NROS_SAFETY_E2E
} // extern "C"

namespace rclcpp {

/// E2E message-integrity status of one received sample (issue 0073): the
/// sequence `gap` since the previous in-order sample, `duplicate`, and
/// `crc_valid` (`1` ok, `0` mismatch, `-1` no CRC on the wire). The C++
/// spelling of the FFI struct (RFC-0089, 2026-10-13: ours-only names take
/// `rclcpp::`), a typedef rather than a mirror so the two can never drift.
using IntegrityStatus = ::nros_cpp_integrity_status_t;

/// Maximum topic name length stored inside a subscription.
/// Mirrors `PUBLISHER_TOPIC_NAME_MAX`. Phase 87.6 thin-wrapper refactor:
/// topic name owned C++-side, not inside a runtime handle.
///
/// Stays in `rclcpp::`: RFC-0089's flip moves the nine TYPES, and this is an
/// implementation bound of one of them, not a name upstream declares.
static constexpr size_t SUBSCRIPTION_TOPIC_NAME_MAX = 256;

} // namespace rclcpp

/// `rclcpp::Node` is named by the friend declaration below and by the
/// out-of-line `Node::create_*` bodies further down. `rclcpp/node.hpp` (included
/// below, after the class, so a consumer pays only for the entities it uses)
/// has the definition; a qualified friend needs the name to EXIST first, which
/// an unqualified `friend class Node;` used to supply implicitly.
///
/// phase-427 W7 — declared in `rclcpp::`, which is where the definition lives.
namespace rclcpp {
class Node;
}

// ============================================================================
// `rclcpp::Subscription<M>` -- DEFINED here (RFC-0089: rclcpp:: is the home)
// ============================================================================
//
// phase-428: the definition moved from `nros::` to `rclcpp::` (the `nros::`
// alias went in phase-483 W1). The nested `SharedPtr` / `ConstSharedPtr` / `UniquePtr`
// aliases live on the class itself, so `rclcpp::Subscription<M>::SharedPtr`
// resolves with no wrapper type in between.
namespace rclcpp {

/// Typed DISPATCH subscription for a ROS 2 topic — phase-456 W2b.
///
/// Mirrors upstream's `rclcpp::Subscription<M>`: the entity is owned by the runtime and
/// the executor calls your handler. The message type `M` must provide
/// `TYPE_NAME`, `TYPE_HASH`, and deserialization support (generated by codegen).
///
/// Usage:
/// ```cpp
/// rclcpp::Subscription<std_msgs::msg::String> sub;
/// RCLCPP_TRY(node.create_subscription(sub, "/chatter", &on_message));
/// rclcpp::spin_once(10);   // `on_message` runs from here
/// ```
///
/// TO TAKE INSTEAD OF BEING CALLED, use `rclcpp::PollSubscription<M>`
/// (`rclcpp/polling_subscription.hpp`), which owns its subscriber and carries
/// `take`, `take_serialized`, `take_validated`, `take_sequence` and
/// `try_borrow`. That API lived on THIS class until phase-456 W2b, for both
/// owners at once: on the dispatch path `storage_` was 656 value-initialized
/// zero bytes the arena never filled, and `take()` handed them to the runtime
/// as an `RmwSubscriber` rather than refusing. One class, two owners, and half
/// the methods answerable only under one of them.
///
/// The name went to the dispatch half because that is what upstream's
/// `Subscription<M>` is — an entity the node owns and the executor drives — and
/// because `Subscription<M>::SharedPtr` is what ported source declares. Note
/// that `rclcpp::Subscription` does carry `take()` upstream (called after a
/// `WaitSet` reports the entity ready); ours is on `PollSubscription<M>`
/// because we have no wait set and the caller-owned subscriber is the thing
/// that can answer it. That trade is ledgered at `cpp:Subscription::take`.
template <typename M> class Subscription {
  public:
    /// `rclcpp::Subscription<M>::SharedPtr` — phase-456 W2.
    ///
    /// `rclcpp::Subscription<M>::SharedPtr member_;` is close to universal in
    /// ported source, so this alias must exist on every target — which
    /// `std::shared_ptr` does not.
    ///
    /// IT IS NOT A POINTER TO A `Subscription<M>`, and that is the point.
    /// `create_subscription` with a callback registers into the executor arena,
    /// which owns the subscriber, the rx buffer, the callback and its capture;
    /// there is no C++ object to point at. What this names is
    /// `rclcpp::SubscriptionHandle<M>` — two words, copyable, and carrying only
    /// the operations a dispatch subscription can actually perform.
    ///
    /// W2 recorded a cost here: the alias and the class had become different
    /// things, because the class still carried a taking API that a handle's
    /// referent could not perform. **phase-456 W2b repaid it** — the taking API
    /// is `rclcpp::PollSubscription<M>`'s now, so the handle's `element_type` IS
    /// `Subscription<M>` again and names a type whose every operation belongs
    /// to a registration. The two are still not the same C++ type, and never
    /// will be: one is a value the caller holds, the other an index into the
    /// arena.
    using SharedPtr = ::rclcpp::SubscriptionHandle<M>;
    /// `rclcpp::Subscription<M>::ConstSharedPtr` — see `SharedPtr`. The same
    /// handle: there is no mutable/const distinction to draw over a registration
    /// that exposes no operation on the entity.
    using ConstSharedPtr = ::rclcpp::SubscriptionHandle<M>;
    /// `rclcpp::Subscription<M>::UniquePtr` — see `SharedPtr`.
    using UniquePtr = ::rclcpp::SubscriptionHandle<M>;

    /// Phase 189.M3.x — typed message-handler signatures for the
    /// *callback-style* subscription (rclcpp dispatch model). The executor
    /// invokes the handler during `spin_once()` on each new sample.
    using TypedSubscriptionFn = void (*)(const M& msg);
    using TypedSubscriptionFnWithCtx = void (*)(const M& msg, void* ctx);
    // Phase 189.M3.4 — callback-with-attachment handler (bridge_origin etc.).
    using TypedSubscriptionInfoFn = void (*)(const M& msg, const uint8_t* attachment,
                                             size_t attachment_len);
    // Phase 269 W3 — callback-with-integrity handler: receives the deserialized
    // message plus the sample's E2E CRC/sequence status. Requires
    // `NROS_SAFETY_E2E` (lowered from `[system].features = ["safety"]`).
#if defined(NROS_SAFETY_E2E)
    using TypedSubscriptionSafetyFn = void (*)(const M& msg,
                                               const ::rclcpp::IntegrityStatus& integrity);
#endif // NROS_SAFETY_E2E

    /// Get the topic name.
    const char* get_topic_name() const { return initialized_ ? topic_name_ : ""; }

    /// Check if the subscription is initialized and valid.
    bool is_valid() const { return initialized_; }

    /// How many publishers are on this subscription's topic, RIGHT NOW —
    /// rclcpp's `Subscription::get_publisher_count`. phase-444.
    ///
    /// The subscription half of `Publisher::get_subscription_count`, which
    /// states the weakening (topic-wide, not matched-to-this-entity), why the
    /// executor and the out-parameter are there, and why an error is not `0`.
    ///
    /// phase-456 W2b split this class in two and this method answers on BOTH
    /// halves, unchanged: it reads `topic_name_`, which each half keeps, and
    /// takes the executor as an argument rather than holding one. A topic-wide
    /// count does not depend on which owner holds the subscriber.
    Result get_publisher_count(::rclcpp::Executor& executor, size_t* out_count) const {
        if (!initialized_) return Result(::rclcpp::ErrorCode::NotInitialized);
        return executor.count_publishers(topic_name_, out_count);
    }

    /// Destructor — there is nothing to release.
    ///
    /// The executor arena owns the subscriber, the rx buffer, the callback and
    /// (phase-456 W1) its capture, and frees them when the executor drops. No
    /// unregister FFI exists, so this cannot remove the registration and does
    /// not pretend to: it clears this object's own bookkeeping. Until
    /// phase-456 W2b the same destructor also freed a POLL subscriber, behind
    /// an `if (initialized_ && !callback_mode_)`; that half moved to
    /// `rclcpp::PollSubscription<M>`, where the condition is unconditional.
    ~Subscription() { initialized_ = false; }

    // Move semantics (non-copyable). Bookkeeping only — there is no storage to
    // relocate, so `nros_cpp_subscription_relocate` is not called here.
    //
    // THE HAZARD, narrowed to its real subject: the out-ref creators below hand
    // the arena `&out` as the trampoline context, so an object registered that
    // way must NOT be moved afterwards — the move copies the handler pointers
    // and leaves the arena dispatching into the source object's address. The
    // returning `create_subscription` (`nros.hpp`) has no such hazard: W1 put
    // the capture in the arena and it hands back a `SharedPtr`, which is a
    // value with nothing pointing at it.
    Subscription(Subscription&& other) : initialized_(other.initialized_) {
        ::memcpy(topic_name_, other.topic_name_, sizeof(topic_name_));
        user_fn_ = other.user_fn_;
        user_fn_ctx_ = other.user_fn_ctx_;
        user_ctx_ = other.user_ctx_;
        user_fn_info_ = other.user_fn_info_;
#if defined(NROS_SAFETY_E2E)
        user_fn_safety_ = other.user_fn_safety_;
#endif // NROS_SAFETY_E2E
        sched_handle_id_ = other.sched_handle_id_;
        executor_ = other.executor_;
        other.initialized_ = false;
    }

    Subscription& operator=(Subscription&& other) {
        if (this != &other) {
            initialized_ = other.initialized_;
            ::memcpy(topic_name_, other.topic_name_, sizeof(topic_name_));
            user_fn_ = other.user_fn_;
            user_fn_ctx_ = other.user_fn_ctx_;
            user_ctx_ = other.user_ctx_;
            user_fn_info_ = other.user_fn_info_;
#if defined(NROS_SAFETY_E2E)
            user_fn_safety_ = other.user_fn_safety_;
#endif // NROS_SAFETY_E2E
            sched_handle_id_ = other.sched_handle_id_;
            executor_ = other.executor_;
            other.initialized_ = false;
        }
        return *this;
    }

    /// Default constructor — creates an unregistered subscription.
    /// Use one of `Node`'s callback-taking `create_subscription` overloads.
    Subscription() : topic_name_{}, initialized_(false) {}

    /// Phase 189.M3.1 — internal: executor `HandleId` usable with
    /// `nros_cpp_bind_handle_to_sched_context`, or `SIZE_MAX` before the
    /// registration has happened.
    ///
    /// phase-456 W2b — this used to be a question worth asking, because the
    /// class also held POLL subscriptions, which register no arena entry and so
    /// left the field at `SIZE_MAX` forever. Every object of this type is an
    /// arena registration now, so a successfully created one always has a
    /// handle; `has_sched_handle()` is false only on a default-constructed
    /// object. `Node` is a friend and sets this on create.
    bool has_sched_handle() const { return sched_handle_id_ != static_cast<size_t>(-1); }
    size_t sched_handle_id() const { return sched_handle_id_; }

    /// The QoS profile this subscription is ACTUALLY running —
    /// `rclcpp::Subscription::get_actual_qos`, no arguments, as upstream's is.
    ///
    /// The phase-467 RMW gap-closure design study's Row 13, closing the gap
    /// phase-456 W2b RECORDED rather than guessed at. The split gave the two
    /// subscription roads two types; `rclcpp::PollSubscription<M>` owns its
    /// subscriber and kept this, and the DISPATCH half — the one a ported
    /// `rclcpp` node holds, and therefore the one where a missing accessor is
    /// a compile error on ported source — lost it, because the FFI needs
    /// `(storage, executor, handle_id)` and this class held only the handle.
    /// The fix is the executor pointer beside it, NOT an argument: adding one
    /// would close the gate and open a divergence on a method that exists to
    /// adopt the no-argument spelling.
    ///
    /// **Per policy, and a policy the backend cannot report is an ABSENCE** —
    /// `ReliabilityPolicy::Unknown`, `DurabilityPolicy::Unknown`, … — never the request echoed
    /// back. See `rclcpp::PollSubscription::get_actual_qos` and
    /// `rclcpp::Publisher::get_actual_qos`, which answer the same question on
    /// their own roads.
    ///
    /// A default-constructed or unregistered subscription answers the
    /// all-absent profile, as the sibling accessors do.
    ::rclcpp::QoS get_actual_qos() const {
        nros_cpp_qos_t f{};
        if (!initialized_ || executor_ == nullptr ||
            nros_cpp_subscription_get_actual_qos(nullptr, executor_, sched_handle_id_, &f) != 0) {
            return ::rclcpp::detail::qos_all_unknown();
        }
        return ::rclcpp::detail::qos_from_ffi(f);
    }

  private:
    Subscription(const Subscription&) = delete;
    Subscription& operator=(const Subscription&) = delete;

    friend class ::rclcpp::Node;

    /// Phase 189.M3.x — raw message trampoline matching `RawSubscriptionCallback`
    /// (`void(data, len, ctx)`). Deserializes the CDR sample into `M` and runs
    /// the user's typed handler. `ctx` is the `Subscription` object (`this`).
    static void message_trampoline(const uint8_t* data, size_t len, void* ctx) {
        auto* self = static_cast<Subscription*>(ctx);
        if (self == nullptr) return;
        M msg;
        if (M::ffi_deserialize(data, len, &msg) != 0) return;
        if (self->user_fn_ != nullptr) {
            self->user_fn_(msg);
        } else if (self->user_fn_ctx_ != nullptr) {
            self->user_fn_ctx_(msg, self->user_ctx_);
        }
    }

    /// Phase 189.M3.4 — trampoline matching `RawSubscriptionInfoCallback`
    /// (`void(data, len, attachment, att_len, ctx)`). Deserializes the CDR sample
    /// into `M` and runs the user's `(const M&, attachment, att_len)` handler.
    static void message_info_trampoline(const uint8_t* data, size_t len, const uint8_t* attachment,
                                        size_t attachment_len, void* ctx) {
        auto* self = static_cast<Subscription*>(ctx);
        if (self == nullptr) return;
        M msg;
        if (M::ffi_deserialize(data, len, &msg) != 0) return;
        if (self->user_fn_info_ != nullptr) {
            self->user_fn_info_(msg, attachment, attachment_len);
        }
    }

#if defined(NROS_SAFETY_E2E)
    /// Phase 269 W3 — trampoline matching `RawSubscriptionSafetyCallback`
    /// (`void(data, len, gap, duplicate, crc_valid, ctx)`). Deserializes the CDR
    /// sample into `M`, packs the three integrity scalars into an
    /// `rclcpp::IntegrityStatus`, and runs the user's
    /// `(const M&, const ::rclcpp::IntegrityStatus&)` handler.
    static void message_safety_trampoline(const uint8_t* data, size_t len, int64_t gap,
                                          bool duplicate, int8_t crc_valid, void* ctx) {
        auto* self = static_cast<Subscription*>(ctx);
        if (self == nullptr) return;
        M msg;
        if (M::ffi_deserialize(data, len, &msg) != 0) return;
        if (self->user_fn_safety_ != nullptr) {
            nros_cpp_integrity_status_t status;
            status.gap = gap;
            status.duplicate = duplicate;
            status.crc_valid = crc_valid;
            self->user_fn_safety_(msg, status);
        }
    }
#endif // NROS_SAFETY_E2E

    /// Copy the topic name into this object's own storage.
    ///
    /// phase-456 W2b — shared by the four arena creators below. Only the POLL
    /// creator had ever filled `topic_name_`, so `get_topic_name()` answered
    /// `""` for every callback-style subscription in the tree: the field was
    /// there, the accessor was there, and nothing wrote it.
    void store_topic_name(const char* topic) {
        size_t n = 0;
        while (topic[n] != '\0' && n + 1 < sizeof(topic_name_)) {
            topic_name_[n] = topic[n];
            ++n;
        }
        topic_name_[n] = '\0';
    }

    // phase-456 W2b — `storage_` (NROS_SUBSCRIBER_SIZE bytes) and `stream_` are
    // GONE, with the taking API that was their only reader. On this path the
    // arena owns the subscriber, so those bytes were never filled, and
    // `take()` reinterpreted them (`&mut *(storage as *mut RmwSubscriber)`)
    // rather than refusing. ~~`callback_mode_`~~ went with them: it distinguished
    // the two owners inside one class, and there is one owner here now.
    char topic_name_[::rclcpp::SUBSCRIPTION_TOPIC_NAME_MAX];
    bool initialized_;
    // Phase 189.M3.1 — executor HandleId for sched-context binding, or
    // SIZE_MAX (the default) when the registration has not happened yet.
    size_t sched_handle_id_ = static_cast<size_t>(-1);
    // The executor that owns the arena entry `sched_handle_id_` indexes —
    // the phase-467 RMW gap-closure design study's Row 13.
    //
    // A `HandleId` is EXECUTOR-SCOPED: an index into one executor's arena, not
    // a process-wide identity, and RFC-0002 puts one executor on one RTOS
    // task, so a tiered image has several. That is why `get_actual_qos()`
    // could not be answered from `sched_handle_id_` alone, and why resolving
    // it "from the sched handle" would have needed a global executor
    // registry. The pair is the same one `rclcpp::Client` already keeps
    // (`{executor_, handle_id_}`), for the same reason.
    void* executor_ = nullptr;
    // The user's handler, dispatched by `message_trampoline` during spin.
    TypedSubscriptionFn user_fn_ = nullptr;
    TypedSubscriptionFnWithCtx user_fn_ctx_ = nullptr;
    TypedSubscriptionInfoFn user_fn_info_ = nullptr;
    void* user_ctx_ = nullptr;
#if defined(NROS_SAFETY_E2E)
    // Phase 269 W3 — handler for the integrity-carrying callback path; nullptr
    // when not using `create_subscription_with_safety`.
    TypedSubscriptionSafetyFn user_fn_safety_ = nullptr;
#endif // NROS_SAFETY_E2E
};

} // namespace rclcpp

// Phase 84.G8: out-of-line definition of Node::create_subscription<M>().
#include "rclcpp/node.hpp"

namespace rclcpp {

// Phase 189.M3.x — callback-style (arena-registered) subscription. The arena
// owns the subscriber + dispatches `out`'s message handler during spin_once, so
// the handle is real and `options.sched_context` is functional. Mirrors the
// callback-style `create_service` one entity over.
} // namespace rclcpp

namespace rclcpp {
template <typename M, typename F, typename>
Result Node::create_subscription(Subscription<M>& out, const char* topic, const ::rclcpp::QoS& qos,
                                 F callback, const ::rclcpp::SubscriptionOptions& options) {
    // RFC-0088 D5 — one image, one backend, one encoding. Compile-time, so a
    // message the linked backend cannot encode never reaches the wire.
    NROS_CPP_ASSERT_MESSAGE_FORMAT(M);
    if (!initialized_) return Result(::rclcpp::ErrorCode::NotInitialized);
    nros_cpp_qos_t ffi_qos = ::rclcpp::detail::qos_to_ffi(qos);

    // Store the user handler (compile error if F isn't convertible to the
    // plain-fn-ptr handler type).
    out.user_fn_ = typename Subscription<M>::TypedSubscriptionFn(callback);
    out.user_fn_ctx_ = nullptr;
    out.user_ctx_ = nullptr;

    uint8_t sched = (options.sched_context == ::rclcpp::SCHED_CONTEXT_UNSET)
                        ? 0u
                        : static_cast<uint8_t>(options.sched_context);
    size_t handle = static_cast<size_t>(-1);
    // phase-402: `sched_context` is a FIELD now. `callback_group` stays unset,
    // i.e. the default group.
    nros_cpp_subscription_options_t ffi_options = nros_cpp_subscription_default_options();
    ffi_options.sched_context = sched;
    // phase-456 W7 — state the bound. `M` is a template parameter of this
    // function, so the number was always available here; leaving it at the
    // option default put this registration on issue 1319's ~~`c_raw_no_hint`~~ row
    // while the sizing descriptor credited the entry with a supplied hint.
    ffi_options.rx_buffer_hint = static_cast<uint32_t>(::rclcpp::rx_buffer_capacity<M>::value);
    nros_cpp_ret_t ret = nros_cpp_subscription_register(
        &handle_, topic, M::TYPE_NAME, M::TYPE_HASH, ffi_qos, &Subscription<M>::message_trampoline,
        &out, &handle, &ffi_options);
    if (ret == 0) {
        out.sched_handle_id_ = handle;
        out.executor_ = executor_handle_;
        // phase-456 W2b — `get_topic_name()` answered "" on every callback-style
        // subscription until this line existed.
        out.store_topic_name(topic);
        out.initialized_ = true;
    }
    return Result(ret);
}
} // namespace rclcpp

namespace rclcpp {

// Phase 273 (RFC-0047) — callback-style subscription **in** a named callback group.
// Mirrors create_subscription (callback-style) exactly but passes group.get_name()
// as `callback_group` so the executor binds the slot via group_sched_table.
} // namespace rclcpp

namespace rclcpp {
template <typename M, typename F, typename>
Result Node::create_subscription_in_group(const ::rclcpp::CallbackGroup& group,
                                          Subscription<M>& out, const char* topic,
                                          const ::rclcpp::QoS& qos, F callback,
                                          const ::rclcpp::SubscriptionOptions& options) {
    // RFC-0088 D5 — one image, one backend, one encoding. Compile-time, so a
    // message the linked backend cannot encode never reaches the wire.
    NROS_CPP_ASSERT_MESSAGE_FORMAT(M);
    if (!initialized_) return Result(::rclcpp::ErrorCode::NotInitialized);
    nros_cpp_qos_t ffi_qos = ::rclcpp::detail::qos_to_ffi(qos);

    out.user_fn_ = typename Subscription<M>::TypedSubscriptionFn(callback);
    out.user_fn_ctx_ = nullptr;
    out.user_ctx_ = nullptr;

    uint8_t sched = (options.sched_context == ::rclcpp::SCHED_CONTEXT_UNSET)
                        ? 0u
                        : static_cast<uint8_t>(options.sched_context);
    size_t handle = static_cast<size_t>(-1);
    // phase-402: both the sched slot and the Phase 273 group name are FIELDS now.
    nros_cpp_subscription_options_t ffi_options = nros_cpp_subscription_default_options();
    ffi_options.sched_context = sched;
    ffi_options.callback_group = group.get_name();
    // phase-456 W7 — see the ungrouped form above. A grouped subscription costs
    // the arena exactly what an ungrouped one does, so it states the same bound.
    ffi_options.rx_buffer_hint = static_cast<uint32_t>(::rclcpp::rx_buffer_capacity<M>::value);
    nros_cpp_ret_t ret = nros_cpp_subscription_register(
        &handle_, topic, M::TYPE_NAME, M::TYPE_HASH, ffi_qos, &Subscription<M>::message_trampoline,
        &out, &handle, &ffi_options);
    if (ret == 0) {
        out.sched_handle_id_ = handle;
        out.executor_ = executor_handle_;
        // phase-456 W2b — `get_topic_name()` answered "" on every callback-style
        // subscription until this line existed.
        out.store_topic_name(topic);
        out.initialized_ = true;
    }
    return Result(ret);
}
} // namespace rclcpp

namespace rclcpp {

// Phase 189.M3.4 — callback-style subscription that delivers the wire attachment.
// Mirrors the callback `create_subscription` one step over, but stores the
// `(const M&, attachment, att_len)` handler + registers via the with-info arena
// path so the trampoline receives the attachment.
} // namespace rclcpp

namespace rclcpp {
template <typename M, typename F, typename>
Result Node::create_subscription_with_info(Subscription<M>& out, const char* topic,
                                           const ::rclcpp::QoS& qos, F callback,
                                           const ::rclcpp::SubscriptionOptions& options) {
    // RFC-0088 D5 — one image, one backend, one encoding. Compile-time, so a
    // message the linked backend cannot encode never reaches the wire.
    NROS_CPP_ASSERT_MESSAGE_FORMAT(M);
    if (!initialized_) return Result(::rclcpp::ErrorCode::NotInitialized);
    nros_cpp_qos_t ffi_qos = ::rclcpp::detail::qos_to_ffi(qos);

    out.user_fn_info_ = typename Subscription<M>::TypedSubscriptionInfoFn(callback);
    out.user_fn_ = nullptr;
    out.user_fn_ctx_ = nullptr;
    out.user_ctx_ = nullptr;

    uint8_t sched = (options.sched_context == ::rclcpp::SCHED_CONTEXT_UNSET)
                        ? 0u
                        : static_cast<uint8_t>(options.sched_context);
    size_t handle = static_cast<size_t>(-1);
    // phase-402: `sched_context` is a FIELD now.
    nros_cpp_subscription_options_t ffi_options = nros_cpp_subscription_default_options();
    ffi_options.sched_context = sched;
    // phase-456 W7 — the attachment rides beside the sample and does not change
    // how many bytes the sample itself needs, so this is the same bound the
    // plain callback form states.
    ffi_options.rx_buffer_hint = static_cast<uint32_t>(::rclcpp::rx_buffer_capacity<M>::value);
    nros_cpp_ret_t ret = nros_cpp_subscription_register_with_info(
        &handle_, topic, M::TYPE_NAME, M::TYPE_HASH, ffi_qos,
        &Subscription<M>::message_info_trampoline, &out, &handle, &ffi_options);
    if (ret == 0) {
        out.sched_handle_id_ = handle;
        out.executor_ = executor_handle_;
        // phase-456 W2b — `get_topic_name()` answered "" on every callback-style
        // subscription until this line existed.
        out.store_topic_name(topic);
        out.initialized_ = true;
    }
    return Result(ret);
}
} // namespace rclcpp

namespace rclcpp {

#if defined(NROS_SAFETY_E2E)
// Phase 269 W3 — out-of-line definition of Node::create_subscription_with_safety.
// Mirrors `create_subscription_with_info` one overload over, but routes through
// `nros_cpp_subscription_register_validated` so the arena dispatches the
// `message_safety_trampoline` on each new sample.
} // namespace rclcpp

namespace rclcpp {
template <typename M, typename F, typename>
Result Node::create_subscription_with_safety(Subscription<M>& out, const char* topic,
                                             const ::rclcpp::QoS& qos, F callback,
                                             const ::rclcpp::SubscriptionOptions& options) {
    // RFC-0088 D5 — one image, one backend, one encoding. Compile-time, so a
    // message the linked backend cannot encode never reaches the wire.
    NROS_CPP_ASSERT_MESSAGE_FORMAT(M);
    if (!initialized_) return Result(::rclcpp::ErrorCode::NotInitialized);
    nros_cpp_qos_t ffi_qos = ::rclcpp::detail::qos_to_ffi(qos);

    out.user_fn_safety_ = typename Subscription<M>::TypedSubscriptionSafetyFn(callback);
    out.user_fn_ = nullptr;
    out.user_fn_ctx_ = nullptr;
    out.user_fn_info_ = nullptr;
    out.user_ctx_ = nullptr;

    uint8_t sched = (options.sched_context == ::rclcpp::SCHED_CONTEXT_UNSET)
                        ? 0u
                        : static_cast<uint8_t>(options.sched_context);
    size_t handle = static_cast<size_t>(-1);
    // phase-402: `sched_context` is a FIELD now.
    nros_cpp_subscription_options_t ffi_options = nros_cpp_subscription_default_options();
    ffi_options.sched_context = sched;
    // phase-456 W7 — the validated path deserializes the same bytes; the safety
    // status is computed from them, not received alongside them.
    ffi_options.rx_buffer_hint = static_cast<uint32_t>(::rclcpp::rx_buffer_capacity<M>::value);
    nros_cpp_ret_t ret = nros_cpp_subscription_register_validated(
        &handle_, topic, M::TYPE_NAME, M::TYPE_HASH, ffi_qos,
        &Subscription<M>::message_safety_trampoline, &out, &handle, &ffi_options);
    if (ret == 0) {
        out.sched_handle_id_ = handle;
        out.executor_ = executor_handle_;
        // phase-456 W2b — `get_topic_name()` answered "" on every callback-style
        // subscription until this line existed.
        out.store_topic_name(topic);
        out.initialized_ = true;
    }
    return Result(ret);
}
} // namespace rclcpp

namespace rclcpp {
#endif // NROS_SAFETY_E2E

namespace detail {

// ---- issue 1995: which callback shapes a subscription takes ------------------
//
// The arena delivers `const M&` from a message decoded in its dispatch frame,
// and that stays the ONE delivery path. Upstream's other common shapes —
// `M::UniquePtr`, `M::SharedPtr`, `M::ConstSharedPtr` (by value or `const&`),
// `std::shared_ptr<const M>` — are taken on a HOSTED build by wrapping the
// callable in an adapter that COPIES the message into a heap object per
// delivery and hands the callback that pointer.
//
// Why a copy, not the service handler's frame loan (`detail::lend_from_frame`):
// a subscription callback KEEPING its pointer (`last_msg_ = msg;`) is the
// common case, not the rare one, and a loan turns it into an abort. A copy
// makes keeping it legal, and the allocation is opted into by the signature —
// a `const M&` callback still allocates nothing. Freestanding builds have no
// allocator on this path, so there the pointer shapes are refused at compile
// time (`NROS_RCLCPP_REFUSE_SUBSCRIPTION_CALLBACK_SHAPE`).

/// Unevaluated-only `declval`, so the detection below needs no `<utility>`.
template <typename T> T&& sub_declval() noexcept;

/// `true` iff `F&` can be called with one argument of type `Arg`.
template <typename F, typename Arg, typename = void>
struct sub_callable_with : ::rclcpp::tr::false_type {};
template <typename F, typename Arg>
struct sub_callable_with<F, Arg, decltype((void)sub_declval<F&>()(sub_declval<Arg>()))>
    : ::rclcpp::tr::true_type {};

/// The shapes, in detection order. `const M&` first, so a callable taking
/// `M` by value or `auto` keeps the allocation-free path.
enum SubscriptionCallbackShape {
    SUB_SHAPE_NONE = 0,
    SUB_SHAPE_REF = 1,              ///< `void(const M&)` — the delivery path itself
    SUB_SHAPE_CONST_SHARED_PTR = 2, ///< `std::shared_ptr<const M>` (`M::ConstSharedPtr`)
    SUB_SHAPE_SHARED_PTR = 3,       ///< `std::shared_ptr<M>` (`M::SharedPtr`)
    SUB_SHAPE_UNIQUE_PTR = 4,       ///< `std::unique_ptr<M>` (`M::UniquePtr`)
};

// Each step instantiates its probe only when every earlier one failed, so a
// generic lambda that IS callable with `const M&` is never instantiated with a
// smart pointer (its body would be a hard error, not a substitution failure).
template <typename M, typename D, bool = sub_callable_with<D, const M&>::value>
struct sub_shape_ref;
#if defined(NROS_CPP_HAS_SHARED_PTR) // hosted-family: shared-ptr-interop
template <typename M, typename D, bool = sub_callable_with<D, ::std::shared_ptr<const M>>::value>
struct sub_shape_const_shared;
template <typename M, typename D, bool = sub_callable_with<D, ::std::shared_ptr<M>>::value>
struct sub_shape_shared;
template <typename M, typename D, bool = sub_callable_with<D, ::std::unique_ptr<M>>::value>
struct sub_shape_unique;
template <typename M, typename D>
struct sub_shape_unique<M, D, true> : ::rclcpp::tr::integral_constant<int, SUB_SHAPE_UNIQUE_PTR> {};
template <typename M, typename D>
struct sub_shape_unique<M, D, false> : ::rclcpp::tr::integral_constant<int, SUB_SHAPE_NONE> {};
template <typename M, typename D>
struct sub_shape_shared<M, D, true> : ::rclcpp::tr::integral_constant<int, SUB_SHAPE_SHARED_PTR> {};
template <typename M, typename D> struct sub_shape_shared<M, D, false> : sub_shape_unique<M, D> {};
template <typename M, typename D>
struct sub_shape_const_shared<M, D, true>
    : ::rclcpp::tr::integral_constant<int, SUB_SHAPE_CONST_SHARED_PTR> {};
template <typename M, typename D>
struct sub_shape_const_shared<M, D, false> : sub_shape_shared<M, D> {};
template <typename M, typename D>
struct sub_shape_ref<M, D, false> : sub_shape_const_shared<M, D> {};
#else
template <typename M, typename D>
struct sub_shape_ref<M, D, false> : ::rclcpp::tr::integral_constant<int, SUB_SHAPE_NONE> {};
#endif // NROS_CPP_HAS_SHARED_PTR
template <typename M, typename D>
struct sub_shape_ref<M, D, true> : ::rclcpp::tr::integral_constant<int, SUB_SHAPE_REF> {};

/// Which `SubscriptionCallbackShape` the callable `F` is, for message `M`.
template <typename M, typename F>
struct subscription_callback_shape : sub_shape_ref<M, typename ::rclcpp::tr::decay<F>::type> {};

/// Turns a callback of shape `Shape` into something `InplaceFn<void(const M&)>`
/// can hold. The `const M&` shape passes through untouched (no wrapper, no
/// allocation); a refused shape becomes a no-op so the refusal's
/// `static_assert` is the only diagnostic the caller reads.
template <typename M, typename F, int Shape> struct subscription_callback_adapter;

template <typename M, typename F> struct subscription_callback_adapter<M, F, SUB_SHAPE_REF> {
    template <typename G> static G&& wrap(G&& f) { return static_cast<G&&>(f); }
};

template <typename M, typename F> struct subscription_callback_adapter<M, F, SUB_SHAPE_NONE> {
    struct Refused {
        void operator()(const M&) {}
    };
    template <typename G> static Refused wrap(G&&) { return Refused(); }
};

#if defined(NROS_CPP_HAS_SHARED_PTR) // hosted-family: shared-ptr-interop
/// The pointer shapes: one heap copy of the delivered message per call. The
/// callback owns (a share of) it and may keep it past the call — upstream's
/// contract, with nothing lent from the dispatch frame.
template <typename M, int Shape> struct subscription_heap_copy {
    static ::std::shared_ptr<M> make(const M& msg) { return ::std::make_shared<M>(msg); }
};
template <typename M> struct subscription_heap_copy<M, SUB_SHAPE_UNIQUE_PTR> {
    static ::std::unique_ptr<M> make(const M& msg) { return ::std::unique_ptr<M>(new M(msg)); }
};

template <typename M, typename F, int Shape> struct subscription_callback_adapter {
    struct Copying {
        typename ::rclcpp::tr::decay<F>::type fn;
        void operator()(const M& msg) { fn(subscription_heap_copy<M, Shape>::make(msg)); }
    };
    template <typename G> static Copying wrap(G&& f) { return Copying{static_cast<G&&>(f)}; }
};
#endif // NROS_CPP_HAS_SHARED_PTR

/// Issue 2036 — `true` iff `F&` can be called with `(A, B)`.
template <typename F, typename A, typename B, typename = void>
struct sub_callable_with2 : ::rclcpp::tr::false_type {};
template <typename F, typename A, typename B>
struct sub_callable_with2<F, A, B,
                          decltype((void)sub_declval<F&>()(sub_declval<A>(), sub_declval<B>()))>
    : ::rclcpp::tr::true_type {};

/// Issue 2036 — the upstream two-argument shapes: the message in one of the
/// four forms above, then `const rclcpp::MessageInfo&`. The values are the
/// one-argument shape's, so `subscription_heap_copy<M, Shape>` serves both.
/// Detected in the same order, for the same reason (a generic lambda is
/// instantiated with a smart pointer only when `const M&` failed).
template <typename M, typename D,
          bool = sub_callable_with2<D, const M&, const ::rclcpp::MessageInfo&>::value>
struct sub_info_shape_ref;
#if defined(NROS_CPP_HAS_SHARED_PTR) // hosted-family: shared-ptr-interop
template <
    typename M, typename D,
    bool = sub_callable_with2<D, ::std::shared_ptr<const M>, const ::rclcpp::MessageInfo&>::value>
struct sub_info_shape_const_shared;
template <typename M, typename D,
          bool = sub_callable_with2<D, ::std::shared_ptr<M>, const ::rclcpp::MessageInfo&>::value>
struct sub_info_shape_shared;
template <typename M, typename D,
          bool = sub_callable_with2<D, ::std::unique_ptr<M>, const ::rclcpp::MessageInfo&>::value>
struct sub_info_shape_unique;
template <typename M, typename D>
struct sub_info_shape_unique<M, D, true>
    : ::rclcpp::tr::integral_constant<int, SUB_SHAPE_UNIQUE_PTR> {};
template <typename M, typename D>
struct sub_info_shape_unique<M, D, false> : ::rclcpp::tr::integral_constant<int, SUB_SHAPE_NONE> {};
template <typename M, typename D>
struct sub_info_shape_shared<M, D, true>
    : ::rclcpp::tr::integral_constant<int, SUB_SHAPE_SHARED_PTR> {};
template <typename M, typename D>
struct sub_info_shape_shared<M, D, false> : sub_info_shape_unique<M, D> {};
template <typename M, typename D>
struct sub_info_shape_const_shared<M, D, true>
    : ::rclcpp::tr::integral_constant<int, SUB_SHAPE_CONST_SHARED_PTR> {};
template <typename M, typename D>
struct sub_info_shape_const_shared<M, D, false> : sub_info_shape_shared<M, D> {};
template <typename M, typename D>
struct sub_info_shape_ref<M, D, false> : sub_info_shape_const_shared<M, D> {};
#else
template <typename M, typename D>
struct sub_info_shape_ref<M, D, false> : ::rclcpp::tr::integral_constant<int, SUB_SHAPE_NONE> {};
#endif // NROS_CPP_HAS_SHARED_PTR
template <typename M, typename D>
struct sub_info_shape_ref<M, D, true> : ::rclcpp::tr::integral_constant<int, SUB_SHAPE_REF> {};

/// Which two-argument (`MessageInfo`) shape `F` is for `M`; `SUB_SHAPE_NONE`
/// when it takes no `MessageInfo`.
template <typename M, typename F>
struct subscription_info_callback_shape
    : sub_info_shape_ref<M, typename ::rclcpp::tr::decay<F>::type> {};

/// Issue 2036 — the MessageInfo the runtime reported, in upstream's layout.
inline ::rclcpp::MessageInfo message_info_from_ffi(const nros_cpp_message_info_t& c) {
    ::rclcpp::RmwMessageInfo r;
    r.source_timestamp = c.source_timestamp;
    r.received_timestamp = c.received_timestamp;
    r.publication_sequence_number = c.publication_sequence_number;
    r.reception_sequence_number = c.reception_sequence_number;
    for (int i = 0; i < 24; ++i)
        r.publisher_gid.data[i] = c.publisher_gid[i];
    // Issue 2074 — carry "the backend reported nothing" through.
    return ::rclcpp::MessageInfo(r, c.has_info);
}

#if defined(NROS_CPP_HAS_SHARED_PTR) // hosted-family: shared-ptr-interop
/// Issue 2036 — a two-argument callback of shape `Shape`, as a callable taking
/// `(const M&, const MessageInfo&)`: the pointer shapes get one heap copy of
/// the message per call, as their one-argument twins do.
template <typename M, typename F, int Shape> struct subscription_info_adapter {
    struct Copying {
        typename ::rclcpp::tr::decay<F>::type fn;
        void operator()(const M& msg, const ::rclcpp::MessageInfo& info) {
            fn(subscription_heap_copy<M, Shape>::make(msg), info);
        }
    };
    template <typename G> static Copying wrap(G&& f) { return Copying{static_cast<G&&>(f)}; }
};
template <typename M, typename F> struct subscription_info_adapter<M, F, SUB_SHAPE_REF> {
    template <typename G> static typename ::rclcpp::tr::decay<G>::type wrap(G&& f) {
        return static_cast<G&&>(f);
    }
};

/// Issue 2036 — register a `(const M&, const MessageInfo&)` callable. HOSTED:
/// the callable lives on the heap and the registration owns it (freed once,
/// when the subscription is released, by the runtime calling `drop`). The
/// runtime takes each sample WITH its metadata
/// (`nros_cpp_subscription_register_message_info`).
template <typename M, typename Fn>
inline Result register_subscription_message_info(::rclcpp::Node& node, const char* topic,
                                                 const QoS& qos, Fn fn, size_t* out_handle_id) {
    const nros_cpp_node_t* h = node.ffi_handle();
    if (h == nullptr) return Result(ErrorCode::NotInitialized);
    Fn* heap = new Fn(static_cast<Fn&&>(fn));
    void (*invoke)(const uint8_t*, size_t, const nros_cpp_message_info_t*, void*) =
        [](const uint8_t* data, size_t len, const nros_cpp_message_info_t* info, void* ctx) {
            M msg;
            if (M::ffi_deserialize(data, len, &msg) != 0 || info == nullptr) return;
            (*static_cast<Fn*>(ctx))(static_cast<const M&>(msg), message_info_from_ffi(*info));
        };
    void (*drop)(void*) = [](void* ctx) { delete static_cast<Fn*>(ctx); };
    nros_cpp_qos_t ffi_qos = detail::qos_to_ffi(qos);
    nros_cpp_subscription_options_t opts = {};
    opts.rx_buffer_hint = static_cast<uint32_t>(::rclcpp::rx_buffer_capacity<M>::value);
    return Result(nros_cpp_subscription_register_message_info(
        h, topic, M::TYPE_NAME, M::TYPE_HASH, ffi_qos, invoke, heap, drop, out_handle_id, &opts));
}
#endif // NROS_CPP_HAS_SHARED_PTR

/// phase-456 W2 — register a capturing callback, with the capture in the ARENA.
///
/// The bridge between `rclcpp::InplaceFn` and
/// `nros_cpp_subscription_register_capturing`: the callable's bytes are handed
/// to the runtime, which copies them into the arena entry and makes the
/// dispatch context its own copy. Nothing of ours survives this call, which is
/// what lets the returned handle be two words.
///
/// The invoker is a non-capturing lambda, so it decays to a plain function
/// pointer — the arena's `callback` field is one, and a capturing trampoline
/// could not be stored there.
template <typename M, typename Fn>
inline Result register_subscription_capturing(::rclcpp::Node& node, const char* topic,
                                              const QoS& qos, const Fn& fn, size_t* out_handle_id) {
    // phase-456 W8 — the per-entry capture budget this used to assert is GONE.
    // It read `sizeof(Fn) <= NROS_CPP_CALLBACK_CAPACITY + 2 * sizeof(void*)` and
    // named ~~`CALLBACK_CAPTURE_BYTES`~~, an arena constant that no longer exists:
    // the runtime allocates exactly `capture_len` bytes now, so there is no
    // cross-language number for the two sides to agree about and nothing here
    // to assert. `InplaceFn`'s own capacity `static_assert` is untouched and is
    // still the knob a too-large CALLABLE hits (`inplace_fn.hpp`), which is a
    // property of that type rather than of this registration.
    const nros_cpp_node_t* h = node.ffi_handle();
    if (h == nullptr) return Result(ErrorCode::NotInitialized);

    nros_cpp_subscription_message_callback_t invoke = [](const uint8_t* data, size_t len,
                                                         void* ctx) {
        // `ctx` is the ARENA's copy of `fn`, not ours.
        const Fn* self = static_cast<const Fn*>(ctx);
        M msg;
        if (M::ffi_deserialize(data, len, &msg) != 0) return;
        (*self)(msg);
    };

    // phase-476 W2 — the arena's copy is destroyed when the subscription is
    // released. `Fn` is an `rclcpp::InplaceFn` (the one caller passes one), so
    // `clear()` runs the stored callable's own destructor.
    void (*destroy)(void*) = [](void* ctx) { static_cast<Fn*>(ctx)->clear(); };

    nros_cpp_qos_t ffi_qos = detail::qos_to_ffi(qos);
    nros_cpp_subscription_options_t opts = {};
    opts.rx_buffer_hint = static_cast<uint32_t>(::rclcpp::rx_buffer_capacity<M>::value);
    return Result(nros_cpp_subscription_register_capturing(
        h, topic, M::TYPE_NAME, M::TYPE_HASH, ffi_qos, invoke,
        reinterpret_cast<const uint8_t*>(&fn), sizeof(Fn), destroy, out_handle_id, &opts));
}

} // namespace detail

} // namespace rclcpp

// ============================================================================
// WHAT LEFT THIS HEADER
// ============================================================================
//
// phase-456 W2 deleted `rclcpp::detail::SubscriptionCallback<M>` — the heap
// cell whose only job was to give a capturing lambda a stable address. W1 put
// the capture in the arena entry, so the cell had nothing left to hold, and
// `nros.hpp`'s returning `create_subscription` stopped allocating one.
//
// phase-456 W2b moved the TAKING API — `take`, `take_sized`, `take_serialized`,
// `take_serialized_with_attachment`, `take_validated`, `take_validated_sized`,
// `take_sequence`, `try_borrow` + `View`, `stream()` and the three QoS-event setters — to
// `rclcpp::PollSubscription<M>` in `rclcpp/polling_subscription.hpp`, together with
// the out-ref creator that fills it. Those calls all dereference `storage_`,
// which only a caller-owned subscriber ever has.

#endif // NROS_RCLCPP_SUBSCRIPTION_HPP
