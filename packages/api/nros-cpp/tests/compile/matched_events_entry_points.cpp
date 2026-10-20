// Issue 1972 — the C++ matched and incompatible-type faces exist with
// rclcpp's names: `MatchedInfo`, `IncompatibleTypeInfo`,
// `PublisherMatchedCallbackType`, `SubscriptionMatchedCallbackType`,
// `IncompatibleTypeCallbackType`, and an `on_matched` / `on_incompatible_type`
// setter on both the publisher and the poll subscription.
//
// Method POINTERS, not calls: syntax-only, no node to create. A missing
// cbindgen declaration (the FFI header is generated) or a drifted signature
// fails here.

#include <rclcpp/rclcpp.hpp>
#include <rclcpp/event.hpp>
#include <stddef.h>
#include <stdint.h>

namespace {

// Mirror of a codegen'd message (as `entity_name_accessors.cpp` does).
struct Payload {
    int32_t value{0};
    static const size_t SERIALIZED_SIZE_MAX = 32;
    static constexpr const char* TYPE_NAME = "test_msgs::msg::dds_::Payload_";
    static constexpr const char* TYPE_HASH = "RIHS01_payload_stub";
    static int ffi_publish(void*, const void*) { return 0; }
    static int ffi_deserialize(const uint8_t*, size_t, Payload*) { return 0; }
    static int ffi_serialize(const Payload*, uint8_t*, size_t, size_t* out) {
        if (out) *out = 0;
        return 0;
    }
};

void on_matched(void* storage, rclcpp::MatchedInfo info, void* ctx) {
    (void)storage;
    (void)ctx;
    int32_t delta = info.current_count_change;
    uint32_t sum = info.total_count + info.total_count_change + info.current_count;
    (void)delta;
    (void)sum;
}

void on_incompatible(void* storage, rclcpp::IncompatibleTypeInfo info, void* ctx) {
    (void)storage;
    (void)ctx;
    (void)(info.total_count + info.total_count_change);
}

} // namespace

int main() {
    rclcpp::PublisherMatchedCallbackType pm = on_matched;
    rclcpp::SubscriptionMatchedCallbackType sm = on_matched;
    rclcpp::IncompatibleTypeCallbackType it = on_incompatible;
    (void)pm;
    (void)sm;
    (void)it;

    using Pub = rclcpp::Publisher<Payload>;
    using Sub = rclcpp::PollSubscription<Payload>;
    rclcpp::Result (Pub::*p1)(rclcpp::PublisherMatchedCallbackType, void*) = &Pub::on_matched;
    rclcpp::Result (Pub::*p2)(rclcpp::IncompatibleTypeCallbackType, void*) =
        &Pub::on_incompatible_type;
    rclcpp::Result (Sub::*s1)(rclcpp::SubscriptionMatchedCallbackType, void*) = &Sub::on_matched;
    rclcpp::Result (Sub::*s2)(rclcpp::IncompatibleTypeCallbackType, void*) =
        &Sub::on_incompatible_type;
    (void)p1;
    (void)p2;
    (void)s1;
    (void)s2;
    return 0;
}
