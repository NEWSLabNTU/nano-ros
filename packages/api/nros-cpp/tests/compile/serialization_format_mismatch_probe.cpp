// RFC-0088 D5 / phase-421 W6 — the NEGATIVE half of the C++ format check.
//
// EXPECTED TO FAIL TO COMPILE. `check-cpp` runs it as an expected-failure, the
// same shape as the C side's `serialization_format_mismatch_probe.c` and as
// `qos_deprecation_probe.cpp`: a clean compile here means the `static_assert` in
// `Node::create_publisher` / `create_subscription` has stopped asserting, so a
// C++ entity could be created over a message in an encoding the linked backend
// does not speak.
//
// The mismatch is spelled the way a non-CDR codegen pack would spell it — the
// message's own `nros::format_of<M>` specialization naming a different format
// from the image's. Every in-tree pack emits CDR today, so stating the
// discriminant here is what makes the refusal testable before a second pack
// exists (RFC-0088 W5 ships one).
#include <nros/nros.hpp>

namespace nros_cpp_serialization_format_mismatch_probe {

// A uORB-encoded message (RFC-0011: the PX4 struct verbatim, no CDR anywhere).
struct VehicleStatus {
    uint64_t timestamp{0};
    static const size_t SERIALIZED_SIZE_MAX = 32;
    static constexpr const char* TYPE_NAME = "px4_msgs::msg::dds_::VehicleStatus_";
    static constexpr const char* TYPE_HASH = "RIHS01_vehicle_status_stub";
    static int ffi_publish(void*, const void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
        if (out) *out = 0;
        return 0;
    }
    static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
};

} // namespace nros_cpp_serialization_format_mismatch_probe

namespace nros {
template <> struct format_of<::nros_cpp_serialization_format_mismatch_probe::VehicleStatus> {
    static constexpr SerializationFormat value = SerializationFormat::Uorb;
};
} // namespace nros

namespace nros_cpp_serialization_format_mismatch_probe {

// If the image itself were ever uORB this probe would be asserting a TRUE
// condition and would compile — reporting a broken check that is in fact fine.
// Fail loudly instead of quietly inverting.
static_assert(::nros::linked_format() == ::nros::SerializationFormat::Cdr,
              "this probe assumes a CDR image; specialize format_of with a format the image "
              "does NOT speak");

// THIS is what must not compile: a typed entity created over a uORB message in
// a CDR image. The assertion lives in `Node::create_publisher`'s body, so the
// instantiation below is what evaluates it.
inline ::nros::Result instantiate(::nros::Node& node) {
    ::nros::Publisher<VehicleStatus> pub;
    return node.create_publisher(pub, "/fmu/out/vehicle_status");
}

} // namespace nros_cpp_serialization_format_mismatch_probe
