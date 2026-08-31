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

} // namespace nros_cpp_rx_size_bound_compile_test
