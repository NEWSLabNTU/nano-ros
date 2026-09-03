// Compile regression for phase-408 W1/W4 (issue 0896): a C++ subscription sizes
// its receive buffer from the subscribed type's OWN derived bound.
//
// The header `-fsyntax-only` loop in `just check cpp` only PARSES templates. The
// seam this phase changed is a template BODY — `nros::bind_subscription<M, C,
// Method>` now passes `nros::rx_size_bound<M>::value` as the `rx_buffer_hint`
// where it passed `M::SERIALIZED_SIZE_MAX`, which is an ESTIMATE that was wrong
// in both directions (issue 0964). So this TU instantiates it, and pins the
// three shapes `rx_size_bound` has to distinguish.
//
// `just check cpp` compiles this with `-fsyntax-only -std=c++14`.
#include <nros/component.hpp>
#include <nros/nros.hpp>

#include <type_traits>

namespace nros_cpp_rx_size_bound_compile_test {

// (1) A codegen'd type WITH a derived bound. The `nros_derived_size_bounds`
// marker and the two nested `*_size_bound` templates are exactly what
// `packs/cpp/message.hpp.jinja` emits.
struct Bounded {
    int32_t data{0};
    static constexpr const char* TYPE_NAME = "p::msg::dds_::Bounded_";
    static constexpr const char* TYPE_HASH = "RIHS01_bounded_stub";
    // The older ESTIMATE, deliberately DIFFERENT from the derived bound below:
    // if anything still reads it for a receive hint, the assertions fail.
    static const size_t SERIALIZED_SIZE_MAX = 1170;
    using nros_derived_size_bounds = void;
    static constexpr size_t TX_MAX_SERIALIZED_SIZE = 133;
    static constexpr size_t RX_MAX_SERIALIZED_SIZE = 137;
    template <class = void> struct tx_size_bound {
        static constexpr size_t value = 133;
    };
    template <class = void> struct rx_size_bound {
        static constexpr size_t value = 137;
    };
    static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
        if (out) *out = 0;
        return 0;
    }
};

// (2) A type from a codegen with no derivation at all, or a hand-written
// message-shaped struct. No marker, so the estimate is all there is and the
// behaviour is unchanged — this is the arm that keeps out-of-tree stubs
// compiling.
struct Legacy {
    int32_t data{0};
    static constexpr const char* TYPE_NAME = "p::msg::dds_::Legacy_";
    static constexpr const char* TYPE_HASH = "RIHS01_legacy_stub";
    static const size_t SERIALIZED_SIZE_MAX = 16;
    static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
        if (out) *out = 0;
        return 0;
    }
};

// (3) A codegen'd type with NO bound: the poison shape. Including it must be
// FINE — that is the whole reason the poison is a class template and not a
// `static constexpr` initializer, which would be evaluated here and break every
// TU that merely includes the header, including one that only publishes.
//
// Asking it for a number is a deliberate `static_assert` failure, so that half
// cannot be asserted in a TU that must compile; it is covered by the emitted
// text in `rosidl-codegen`'s golden corpus instead.
struct Unbounded {
    int32_t data{0};
    static constexpr const char* TYPE_NAME = "p::msg::dds_::Unbounded_";
    static constexpr const char* TYPE_HASH = "RIHS01_unbounded_stub";
    static const size_t SERIALIZED_SIZE_MAX = 264;
    using nros_derived_size_bounds = void;
    template <class NROS_size_bound_required = void> struct tx_size_bound {
        static_assert(::nros::detail::size_bound_dependent_false<NROS_size_bound_required>::value,
                      "NROS_UNBOUNDED__p_msg_unbounded__field_data: p/Unbounded states no "
                      "serialized-size bound -- unbounded member: data (string).");
        static constexpr size_t value = 0;
    };
    template <class NROS_size_bound_required = void> struct rx_size_bound {
        static_assert(::nros::detail::size_bound_dependent_false<NROS_size_bound_required>::value,
                      "NROS_UNBOUNDED__p_msg_unbounded__field_data: p/Unbounded states no "
                      "serialized-size bound -- unbounded member: data (string).");
        static constexpr size_t value = 0;
    };
    static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
        if (out) *out = 0;
        return 0;
    }
};

static_assert(::nros::rx_size_bound<Bounded>::value == 137,
              "a derived type's receive hint is RX_MAX_SERIALIZED_SIZE -- max(XCDR1, XCDR2), "
              "never the SERIALIZED_SIZE_MAX estimate");
static_assert(::nros::tx_size_bound<Bounded>::value == 133,
              "a derived type's transmit bound is TX_MAX_SERIALIZED_SIZE -- XCDR1");
static_assert(::nros::rx_size_bound<Legacy>::value == 16,
              "a type with no derivation keeps the pre-phase-408 behaviour");

class Listener {
    void on_msg(const Bounded&) {}

  public:
    // Instantiates the changed template BODY: the hint reaching
    // `create_subscription_raw` is the derived bound.
    ::nros::Result configure(::nros::Node& node) {
        ::nros::Result r =
            ::nros::bind_subscription<Bounded, Listener, &Listener::on_msg>(node, "/chatter", this);
        if (!r.ok()) return r;
        // The escape hatch, for a type with no bound or a deliberate override.
        return ::nros::bind_subscription_sized<Bounded, Listener, &Listener::on_msg>(
            node, "/chatter_sized", this, 2048);
    }
};

inline ::nros::Result instantiate(::nros::Node& node) {
    static Listener listener;
    (void)sizeof(Unbounded); // included, never asked for its bound
    return listener.configure(node);
}

// ===========================================================================
// issue 0964 — the RECEIVE buffers inside these headers stop sizing themselves
// from the estimate.
//
// `nros::rx_buffer_capacity<M>` is what the ~13 receive sites now stack, and it
// selects its arm with the SAME `detail::shape_of<M>()` predicate as
// `rx_size_bound<M>` — one question, two answers, so the two cannot disagree
// about which arm a type is in. It differs from `rx_size_bound<M>` in exactly
// one place: the `unbounded` arm falls back to the estimate instead of
// poisoning, because flipping those sites to a compile error is a product
// decision this issue leaves open.
// ===========================================================================

static_assert(::nros::has_derived_size_bound<Bounded>::value,
              "a type that states RX/TX_MAX_SERIALIZED_SIZE has a derived bound");
static_assert(!::nros::has_derived_size_bound<Legacy>::value,
              "no marker at all is NOT the same fact as 'marked, and has no bound'");
static_assert(!::nros::has_derived_size_bound<Unbounded>::value,
              "marked but stating no constants means the bound was computed and does not exist");

static_assert(::nros::rx_buffer_capacity<Bounded>::value == 137,
              "a bounded type's receive buffer is RX_MAX_SERIALIZED_SIZE -- max(XCDR1, XCDR2), "
              "never the 1170-byte SERIALIZED_SIZE_MAX estimate beside it");
static_assert(::nros::tx_buffer_capacity<Bounded>::value == 133,
              "a bounded type's transmit capacity is TX_MAX_SERIALIZED_SIZE -- XCDR1");
static_assert(::nros::rx_buffer_capacity<Legacy>::value == Legacy::SERIALIZED_SIZE_MAX,
              "a type with no derivation keeps the pre-phase-408 behaviour");
static_assert(::nros::rx_buffer_capacity<Unbounded>::value == Unbounded::SERIALIZED_SIZE_MAX,
              "issue 0964 step 3, PINNED so a future flip is a deliberate edit here: a type with "
              "NO bound stays on the estimate at a receive site rather than becoming a compile "
              "error. `rx_size_bound<Unbounded>` is still the poison -- that is the difference "
              "between the two traits, and the whole reason both exist");

// Anti-drift, the 0088-family rule: `shape_of` decides `derived` by probing the
// CONSTANTS, while the poison arm reads the nested TEMPLATES. Both come out of
// one `{% if tx_max_serialized_size %}` in `packs/cpp/message.hpp.jinja`, so for
// a bounded type the two spellings must return the same number. If codegen ever
// emits one without the other, this is what says so.
static_assert(::nros::rx_size_bound<Bounded>::value == Bounded::rx_size_bound<>::value,
              "the constant and the nested template are one emitted fact");
static_assert(::nros::tx_size_bound<Bounded>::value == Bounded::tx_size_bound<>::value,
              "the constant and the nested template are one emitted fact");
static_assert(::nros::rx_buffer_capacity<Bounded>::value == ::nros::rx_size_bound<Bounded>::value,
              "where a derived bound EXISTS the two traits agree -- the capacity trait is a "
              "fallback for the types that have none, not a second opinion for the ones that do");

// Generated-shape service and action types over the three payload shapes, so
// the receive paths on `Client`, `Service`, `ActionClient` and the polling
// tiers get instantiated for a bounded type AND for an unbounded one.
template <class Payload> struct SvcOf {
    using Request = Payload;
    using Response = Payload;
    static constexpr const char* TYPE_NAME = "p::srv::dds_::Svc_";
    static constexpr const char* TYPE_HASH = "RIHS01_svc_stub";
};
template <class Payload> struct ActionOf {
    using Goal = Payload;
    using Result = Payload;
    using Feedback = Payload;
    static constexpr const char* TYPE_NAME = "p::action::dds_::Act_";
    static constexpr const char* TYPE_HASH = "RIHS01_act_stub";
};

// Every RECEIVE site issue 0964 enumerates, instantiated. Taken by reference:
// constructing these needs a Node, and the point is the template BODY.
template <class M>
inline ::nros::Result recv_paths(::nros::Subscription<M>& sub, ::nros::Stream<M>& stream, M& msg) {
    (void)sub.try_recv(msg);                  // subscription.hpp -- derived where one exists
    (void)sub.template take_sized<4096>(msg); // ... and the escape hatch
    nros_cpp_integrity_status_t status{};
    (void)sub.try_recv_validated(msg, status);
    (void)sub.template take_validated_sized<4096>(msg, status);
    (void)stream.try_next(msg); // stream.hpp
    (void)stream.template try_next_sized<4096>(msg);
    (void)stream.wait_next(nullptr, 1, msg);
    (void)stream.template wait_next_sized<4096>(nullptr, 1, msg);
    return ::nros::Result::success();
}

template <class M>
inline ::nros::Result client_paths(::nros::Client<SvcOf<M>>& client,
                                   ::nros::Service<SvcOf<M>>& service, ::nros::TickCtx& tick,
                                   M& payload) {
    (void)client.send_request(payload); // future.hpp -- Future<T>'s cached_buf_
    (void)client.template send_request_sized<4096>(payload);
    (void)client.call(payload, payload, 1);
    (void)client.template call_sized<4096>(payload, payload, 1);
    (void)client.call_polling(payload, payload, 1); // client.hpp resp_buf
    (void)client.template call_polling_sized<4096>(payload, payload, 1);
    int64_t seq = 0;
    (void)service.try_recv_request(payload, seq); // service.hpp -- a RECEIVE buffer
    (void)service.template try_recv_request_sized<4096>(payload, seq);
    (void)tick.template call<M, M>("e", payload, payload); // tick_ctx.hpp resp_buf
    (void)tick.template call_sized<M, M, 4096>("e", payload, payload);
    return ::nros::Result::success();
}

template <class M>
inline ::nros::Result action_paths(::nros::ActionClient<ActionOf<M>>& client,
                                   ::nros::PollingActionClient<ActionOf<M>>& polling_client,
                                   ::nros::PollingActionServer<ActionOf<M>>& polling_server,
                                   M& payload) {
    uint8_t goal_id[16] = {0};
    (void)client.get_result(goal_id, payload); // action_client.hpp result
    (void)client.template get_result_sized<4096>(goal_id, payload);
    (void)client.get_result_future(goal_id);
    (void)client.template get_result_future_sized<4096>(goal_id);
    (void)client.try_recv_feedback(payload); // action_client.hpp feedback
    (void)client.template try_recv_feedback_sized<4096>(payload);
    (void)polling_client.try_recv_result(payload); // polling_action_client.hpp result
    (void)polling_client.template try_recv_result_sized<4096>(payload);
    (void)polling_client.try_recv_feedback(goal_id, payload);
    (void)polling_client.template try_recv_feedback_sized<4096>(goal_id, payload);
    int64_t seq = 0;
    (void)polling_server.try_recv_goal_request(goal_id, payload, seq); // polling server goal
    (void)polling_server.template try_recv_goal_request_sized<4096>(goal_id, payload, seq);
    return ::nros::Result::success();
}

// A SITE-level assertion, not just a trait-level one. `Future<T, Cap>` carries
// its receive capacity in its TYPE, so what number `Client<S>::send_request`
// actually spends is observable here: 137 is the derived RX bound, 1170 is the
// estimate this issue is about. Reverting that one call site to the estimate --
// with `rx_buffer_capacity` left entirely correct -- fails THIS line and
// nothing else.
static_assert(
    std::is_same<decltype(std::declval<::nros::Client<SvcOf<Bounded>>&>().send_request(
                     std::declval<const Bounded&>())),
                 ::nros::Future<Bounded, 137>>::value,
    "Client<S>::send_request must hand back a Future sized from the response type's DERIVED "
    "bound, not from its SERIALIZED_SIZE_MAX estimate");
static_assert(std::is_same<decltype(std::declval<::nros::Client<SvcOf<Unbounded>>&>().send_request(
                               std::declval<const Unbounded&>())),
                           ::nros::Future<Unbounded, Unbounded::SERIALIZED_SIZE_MAX>>::value,
              "a response type with NO derived bound keeps the estimate -- issue 0964 step 3");

/// The point of the whole arrangement: the SAME bodies instantiate for a type
/// with a derived bound and for one with none. Before this, a blanket switch to
/// `rx_size_bound<M>` would have made the second column a compile error for 81
/// of the 120 stock Humble types.
inline ::nros::Result instantiate_recv_paths(
    ::nros::Subscription<Bounded>& bsub, ::nros::Stream<Bounded>& bstream, Bounded& bmsg,
    ::nros::Subscription<Unbounded>& usub, ::nros::Stream<Unbounded>& ustream, Unbounded& umsg,
    ::nros::Client<SvcOf<Bounded>>& bclient, ::nros::Service<SvcOf<Bounded>>& bservice,
    ::nros::Client<SvcOf<Unbounded>>& uclient, ::nros::Service<SvcOf<Unbounded>>& uservice,
    ::nros::TickCtx& tick, ::nros::ActionClient<ActionOf<Bounded>>& bac,
    ::nros::PollingActionClient<ActionOf<Bounded>>& bpac,
    ::nros::PollingActionServer<ActionOf<Bounded>>& bpas,
    ::nros::ActionClient<ActionOf<Unbounded>>& uac,
    ::nros::PollingActionClient<ActionOf<Unbounded>>& upac,
    ::nros::PollingActionServer<ActionOf<Unbounded>>& upas) {
    (void)recv_paths<Bounded>(bsub, bstream, bmsg);
    (void)recv_paths<Unbounded>(usub, ustream, umsg);
    (void)client_paths<Bounded>(bclient, bservice, tick, bmsg);
    (void)client_paths<Unbounded>(uclient, uservice, tick, umsg);
    (void)action_paths<Bounded>(bac, bpac, bpas, bmsg);
    (void)action_paths<Unbounded>(uac, upac, upas, umsg);
    return ::nros::Result::success();
}

} // namespace nros_cpp_rx_size_bound_compile_test
