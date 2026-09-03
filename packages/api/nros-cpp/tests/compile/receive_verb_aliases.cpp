// Phase 379 W6 decision 1 — the C++ non-blocking receive verb under its rclcpp
// name, and the old spellings kept alive as `[[deprecated]]` forwarders.
//
// `Subscription<M>::try_recv` and `rclcpp::Subscription::take` are THE SAME
// OPERATION — non-blocking, consuming, reporting emptiness without failing —
// so the ledger's `c:take` / `cpp:Subscription::take` /
// `cpp:Subscription::take_serialized` / `cpp:Service::take_request` rows are a
// naming defect, not a platform divergence. `_raw` becomes `_serialized`
// because that is ROS 2's word for the pre-CDR byte form
// (`rcl_take_serialized_message`).
//
// The header `-fsyntax-only` loop in `just check cpp` only PARSES the
// templates; it does not instantiate them. This TU instantiates both classes
// against a message / service type matching the generated shape, so the method
// BODIES are type-checked — a forwarder whose argument list drifted from the
// method it forwards to fails HERE.
//
// The `-Werror=deprecated-declarations` half is `receive_deprecation_probe.cpp`.
// This file is compiled with `-Wno-deprecated-declarations`, because it names
// the deprecated spellings on purpose.

#include <nros/nros.hpp>

#include <type_traits>

namespace nros_cpp_receive_verb_alias_test {

// Mirror of a codegen'd message (cf. std_msgs/msg/Int32).
struct Int32 {
    int32_t data{0};
    static const size_t SERIALIZED_SIZE_MAX = 16;
    static constexpr const char* TYPE_NAME = "std_msgs::msg::dds_::Int32_";
    static constexpr const char* TYPE_HASH = "RIHS01_int32_stub";
    static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
        if (out) *out = 0;
        return 0;
    }
};

// Mirror of a codegen'd service (cf. example_interfaces/srv/AddTwoInts).
struct AddTwoInts {
    using Request = Int32;
    using Response = Int32;
    static constexpr const char* TYPE_NAME = "example_interfaces::srv::dds_::AddTwoInts_";
    static constexpr const char* TYPE_HASH = "RIHS01_addtwoints_stub";
};

// 1. The renamed methods exist and their bodies type-check.
inline ::nros::Result instantiate_new(::nros::Subscription<Int32>& sub,
                                      ::nros::Service<AddTwoInts>& srv) {
    Int32 msg{};
    uint8_t buf[64];
    uint8_t att[16];
    size_t len = 0;
    size_t att_len = 0;
    size_t lens[4];
    size_t count = 0;
    int64_t seq = 0;
    nros_cpp_integrity_status_t status{};

    ::nros::Result r = sub.take(msg);
    (void)sub.take_serialized(buf, sizeof(buf), len);
    (void)sub.take_serialized_with_attachment(buf, sizeof(buf), len, att, sizeof(att), att_len);
    (void)sub.take_sequence(buf, 16, 4, lens, count);
    (void)sub.take_validated(msg, status);
    (void)srv.take_request(msg, seq);
    return r;
}

// 2. The deprecated spellings still resolve, with the same signatures.
#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wdeprecated-declarations"
#endif
inline ::nros::Result instantiate_old(::nros::Subscription<Int32>& sub,
                                      ::nros::Service<AddTwoInts>& srv) {
    Int32 msg{};
    uint8_t buf[64];
    uint8_t att[16];
    size_t len = 0;
    size_t att_len = 0;
    size_t lens[4];
    size_t count = 0;
    int64_t seq = 0;
    nros_cpp_integrity_status_t status{};

    ::nros::Result r = sub.try_recv(msg);
    (void)sub.try_recv_raw(buf, sizeof(buf), len);
    (void)sub.try_recv_raw_with_attachment(buf, sizeof(buf), len, att, sizeof(att), att_len);
    (void)sub.try_recv_sequence(buf, 16, 4, lens, count);
    (void)sub.try_recv_validated(msg, status);
    (void)srv.try_recv_request(msg, seq);
    return r;
}

// The forwarders must return exactly what they forward to — a `Result` that
// became a `bool` on the way through would pass a name check.
static_assert(std::is_same<decltype(std::declval<::nros::Subscription<Int32>&>().try_recv(
                               std::declval<Int32&>())),
                           ::nros::Result>::value,
              "Subscription<M>::try_recv must still return nros::Result");
static_assert(std::is_same<decltype(std::declval<::nros::Subscription<Int32>&>().take(
                               std::declval<Int32&>())),
                           ::nros::Result>::value,
              "Subscription<M>::take must return nros::Result");
#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic pop
#endif

} // namespace nros_cpp_receive_verb_alias_test
