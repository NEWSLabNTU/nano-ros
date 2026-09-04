// RFC-0088 D5 / phase-421 W6 — the POSITIVE half of the C++ format check.
//
// `nros::format_of<M>` is what a message type answers; `nros::linked_format()`
// is what the image's backend speaks; `NROS_CPP_ASSERT_MESSAGE_FORMAT(M)` is
// the `static_assert` every typed entity-creation funnel expands. The header
// `-fsyntax-only` loop in `just check cpp` only PARSES the templates, so this TU
// INSTANTIATES `create_publisher` / `create_subscription` against a message type
// carrying the specialization codegen emits, which is the only way the assert in
// their bodies is actually evaluated.
//
// The MISMATCH is the other half and must FAIL to compile:
// `serialization_format_mismatch_probe.cpp`, run as an expected-failure.
//
// `just check cpp` compiles this with `-fsyntax-only -std=c++14`.
#include <nros/nros.hpp>

#include <type_traits>

namespace nros_cpp_serialization_format_compile_test {

// Mirror of a codegen'd message (cf. std_msgs/msg/Int32).
struct Int32 {
    int32_t data{0};
    static const size_t SERIALIZED_SIZE_MAX = 16;
    static constexpr const char* TYPE_NAME = "std_msgs::msg::dds_::Int32_";
    static constexpr const char* TYPE_HASH = "RIHS01_int32_stub";
    static int ffi_publish(void*, const void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
        if (out) *out = 0;
        return 0;
    }
    static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
};

} // namespace nros_cpp_serialization_format_compile_test

// The specialization `packs/cpp/message.hpp.jinja` emits, verbatim in shape.
namespace nros {
template <> struct format_of<::nros_cpp_serialization_format_compile_test::Int32> {
    static constexpr SerializationFormat value = SerializationFormat::Cdr;
};
} // namespace nros

namespace nros_cpp_serialization_format_compile_test {

// The enum is `uint8_t`-backed (RFC-0088 D5) — a wider one would not match the
// `nros_serdes::format::SerializationFormatId` `#[repr(u8)]` it mirrors.
static_assert(
    std::is_same<typename std::underlying_type<::nros::SerializationFormat>::type, uint8_t>::value,
    "nros::SerializationFormat must be uint8_t-backed to mirror the Rust repr(u8)");

// The name is a `const char*`, never a `std::string`/`std::string_view`: these
// headers compile against Zephyr's minimal libcpp, where `<string>` does not
// exist (issue 0112). A regression to a std type stops compiling here.
static_assert(std::is_same<decltype(::nros::linked_format_name()), const char*>::value,
              "nros::linked_format_name() must return const char* (issue 0112)");

// A message with no specialization still ANSWERS — the C++ mirror of the
// defaulted `nros_core::RosMessage::SERIALIZATION_FORMAT_ID` (RFC-0088 D1).
struct Unspecialized {};
static_assert(::nros::format_of<Unspecialized>::value == ::nros::SerializationFormat::Cdr,
              "format_of<M> must default to Cdr, mirroring the defaulted Rust const");

// Force instantiation of the entity creators, so the `static_assert` inside
// their bodies is evaluated rather than merely parsed.
inline ::nros::Result instantiate(::nros::Node& node) {
    ::nros::Publisher<Int32> pub;
    ::nros::Result rp = node.create_publisher(pub, "/count");

    ::nros::Subscription<Int32> sub;
    ::nros::Result rs = node.create_subscription(sub, "/count");

    ::nros::PollingSubscription<Int32> poll;
    ::nros::Result rq = node.create_polling_subscription(poll, "/count");

    (void)rs;
    (void)rq;
    return rp;
}

} // namespace nros_cpp_serialization_format_compile_test
