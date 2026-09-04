// nros-cpp: the serialization format as a compile-time fact (freestanding C++14)
//
// RFC-0088 D5. ROS 2 asks `rmw_get_serialization_format()` at run time because
// it resolves its typesupport through `dlopen`; nano-ros does not `dlopen`, so
// the answer is already known when the image is compiled — one image links one
// backend, and one backend speaks one encoding.
//
// `const char*` throughout, never `std::string` or `std::string_view`: these
// headers must compile against Zephyr's minimal libcpp behind `NROS_CPP_STD`,
// where `<string>` does not exist (issue 0112).

/**
 * @file serialization_format.hpp
 * @ingroup grp_support
 * @brief `nros::format_of<M>` — a message type's serialization format;
 *        `nros::linked_format()` — the linked backend's; and the
 *        `static_assert` that refuses to create a typed entity over a message
 *        the backend cannot encode.
 */

#ifndef NROS_CPP_SERIALIZATION_FORMAT_HPP
#define NROS_CPP_SERIALIZATION_FORMAT_HPP

#include <cstdint>

// `NROS_CPP_SERIALIZATION_FORMAT_ID` — cbindgen output, lowered from
// `nros_cpp`'s own mirror, which a `const _` proves equal to
// `nros_node::session::IMAGE_SERIALIZATION_FORMAT_ID`. Taken from the C++ FFI
// header rather than from `<nros/nros_generated.h>` so that a generated message
// header never pulls a C API header ahead of this one (issue 0160 — the FFI
// struct mirrors make that include order one-way).
#include "nros_cpp_ffi.h"

namespace nros {

/// The serialization formats this image can name.
///
/// RFC-0088 D2 — the discriminant is **image-local**. It is assigned per build
/// from the formats declared in that image; the low values below are reserved
/// for the in-tree formats for readability and nothing more. Never persist one,
/// and never compare one against a value another image produced — the *string*
/// (`linked_format_name()`) is the identity that crosses an image boundary.
enum class SerializationFormat : uint8_t {
    /// OMG CDR as ROS 2 puts it on the wire, encapsulation header included.
    Cdr = 1,
    /// PX4's in-memory struct, verbatim — no encoding step at all (RFC-0011).
    Uorb = 2,
};

/// The serialization format a message type is encoded in.
///
/// **Defaulted to CDR**, deliberately: this is the C++ mirror of
/// `nros_core::RosMessage::SERIALIZATION_FORMAT_ID`, which RFC-0088 D1 makes a
/// *defaulted* const so that every message answers something and the 142
/// existing implementors cost nothing. Codegen specializes it per message
/// anyway, so a generated type STATES its format rather than inheriting it;
/// the default is what keeps a hand-written tag type (the PX4 uORB demo's
/// `DebugKeyValueTag`, the compile fixtures' `Bounded`/`Unbounded`) usable
/// without a specialization it has no generator to write.
template <typename M> struct format_of {
    static constexpr SerializationFormat value = SerializationFormat::Cdr;
};

/// The format the backend this image links speaks.
///
/// **Only meaningful in a single-backend image.** A bridge image links two
/// backends and has no single answer; it asks per session instead, with
/// `nros::Node::serialization_format()`. `check-format-macro-scope` refuses a
/// bridge-linked translation unit that reaches the underlying macro.
constexpr SerializationFormat linked_format() {
    return static_cast<SerializationFormat>(NROS_CPP_SERIALIZATION_FORMAT_ID);
}

/// The cross-image identity string of `linked_format()` (RFC-0088 D2).
///
/// `const char*`, so the header stays usable against Zephyr's minimal libcpp.
/// Derived from the discriminant rather than emitted beside it: cbindgen maps
/// no Rust `&str` to a C constant, so a second generated macro would be a
/// second authored spelling with nothing tying the two together.
constexpr const char* linked_format_name() {
    return linked_format() == SerializationFormat::Cdr
               ? "cdr"
               : (linked_format() == SerializationFormat::Uorb ? "uorb" : "unknown");
}

} // namespace nros

/**
 * Assert that message type @p M is encoded in the format the linked backend
 * speaks. Expanded at every typed entity-creation site — the C++ sibling of
 * `nros_node::format_check::assert_message_format`.
 *
 * A macro, not a function, so the failing `static_assert` reports the file and
 * line of the `create_publisher` / `create_subscription` call rather than a
 * line inside this header.
 */
#define NROS_CPP_ASSERT_MESSAGE_FORMAT(M)                                                          \
    static_assert(::nros::format_of<M>::value == ::nros::linked_format(),                          \
                  "RFC-0088: this message type is not encoded in the format the linked "           \
                  "backend speaks: one image, one backend, one encoding. A bridge image "          \
                  "is the only place two formats legitimately meet, and it converts "              \
                  "explicitly.")

#endif // NROS_CPP_SERIALIZATION_FORMAT_HPP
