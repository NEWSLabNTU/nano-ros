// issue 1995 — upstream's pointer-shaped subscription callbacks, on a hosted
// build. RUNTIME, because the contract is behaviour, not a signature:
//
//   (1) every shape CLASSIFIES as the one it is — `const M&` (and by-value,
//       and a generic lambda) keeps the allocation-free path; `M::UniquePtr`,
//       `M::SharedPtr`, `M::ConstSharedPtr`, `const M::SharedPtr&`,
//       `std::shared_ptr<const M>` and a `std::bind` over a member taking
//       `ConstSharedPtr` take the copying adapter; a callable taking none of
//       them is NONE (refused at `create_subscription`);
//   (2) through the SAME `InplaceFn<void(const M&)>` the arena stores, each
//       pointer callback sees the delivered message's contents;
//   (3) a callback that KEEPS its pointer keeps a live, independent copy — the
//       delivering frame's message can die and be overwritten, and the kept
//       pointer still reads what was delivered. This is the difference from
//       the service handler's frame loan, which aborts on retention.
//
// Header-only (the adapter lives in `rclcpp/subscription.hpp`), so it links no
// nano-ros archive; `--unresolved-symbols=ignore-all` covers only the
// issue-0360 variant anchors, as for `service_shared_ptr_handler_runtime.cpp`.
// The end-to-end path — `create_subscription` registering an adapted callback
// on the stub backend — is the `rclcpp-examples` fixture's
// `minimal_subscriber_lambda` (UniquePtr) and `multithreaded_executor`
// (`std::bind` over ConstSharedPtr members), compiled, linked and booted.

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <functional>
#include <memory>

#include <rclcpp/rclcpp.hpp>

namespace {

// A message in the codegen's shape, aliases included (see
// `packs/cpp/_pointer_aliases.jinja`).
struct Msg {
#if defined(NROS_CPP_HAS_SHARED_PTR) // hosted-family: shared-ptr-interop
    using SharedPtr = ::std::shared_ptr<Msg>;
    using ConstSharedPtr = ::std::shared_ptr<const Msg>;
    using UniquePtr = ::std::unique_ptr<Msg>;
#endif
    int32_t value = 0;
    char text[16] = {};
    static constexpr const char* TYPE_NAME = "test_msgs::msg::dds_::Msg_";
    static constexpr const char* TYPE_HASH = "RIHS01_stub";
    static const size_t SERIALIZED_SIZE_MAX = sizeof(int32_t) + 16;
};

void expect(bool cond, const char* what) {
    if (!cond) {
        ::std::fprintf(stderr, "subscription_pointer_callback_runtime: FAILED: %s\n", what);
        ::std::abort();
    }
}

namespace d = ::rclcpp::detail;

template <typename F> constexpr int shape_of() {
    return d::subscription_callback_shape<Msg, F>::value;
}

/// What the arena does: hold the adapted callable as `InplaceFn<void(const M&)>`
/// and call it with a message living in the delivering frame.
template <typename F> void deliver(F&& cb, int32_t value, const char* text) {
    constexpr int shape = shape_of<F>();
    ::rclcpp::InplaceFn<void(const Msg&)> held(
        d::subscription_callback_adapter<Msg, F, shape>::wrap(static_cast<F&&>(cb)));
    Msg frame;
    frame.value = value;
    ::std::strncpy(frame.text, text, sizeof(frame.text) - 1);
    held(frame);
    // The frame message dies here; scribble over it first so a pointer INTO it
    // would read garbage rather than happen to still hold the right bytes.
    ::std::memset(&frame, 0xA5, sizeof(frame));
    held.clear();
}

int32_t g_seen = -1;

struct Node {
    Msg::ConstSharedPtr last_;
    void member_cb(const Msg::ConstSharedPtr msg) {
        g_seen = msg->value;
        last_ = msg; // upstream's common pattern: keep the latest message
    }
};

} // namespace

int main() {
    // (1) Classification.
    auto by_ref = [](const Msg& m) { g_seen = m.value; };
    auto by_value = [](Msg m) { g_seen = m.value; };
    auto generic = [](const auto& m) { g_seen = m.value; };
    auto unique = [](Msg::UniquePtr m) { g_seen = m->value; };
    auto shared = [](Msg::SharedPtr m) { g_seen = m->value; };
    auto shared_cref = [](const Msg::SharedPtr& m) { g_seen = m->value; };
    auto const_shared = [](const Msg::ConstSharedPtr m) { g_seen = m->value; };
    auto std_const_shared = [](std::shared_ptr<const Msg> m) { g_seen = m->value; };
    auto wrong = [](int) {};
    static_assert(shape_of<decltype(by_ref)>() == d::SUB_SHAPE_REF, "const M& is the plain path");
    static_assert(shape_of<decltype(by_value)>() == d::SUB_SHAPE_REF, "by-value stays plain");
    static_assert(shape_of<decltype(generic)>() == d::SUB_SHAPE_REF, "generic lambda stays plain");
    static_assert(shape_of<decltype(unique)>() == d::SUB_SHAPE_UNIQUE_PTR, "M::UniquePtr");
    static_assert(shape_of<decltype(shared)>() == d::SUB_SHAPE_SHARED_PTR, "M::SharedPtr");
    static_assert(shape_of<decltype(shared_cref)>() == d::SUB_SHAPE_SHARED_PTR,
                  "const M::SharedPtr&");
    static_assert(shape_of<decltype(const_shared)>() == d::SUB_SHAPE_CONST_SHARED_PTR,
                  "M::ConstSharedPtr");
    static_assert(shape_of<decltype(std_const_shared)>() == d::SUB_SHAPE_CONST_SHARED_PTR,
                  "std::shared_ptr<const M>");
    static_assert(shape_of<void (*)(const Msg&)>() == d::SUB_SHAPE_REF, "function pointer");
    static_assert(shape_of<decltype(wrong)>() == d::SUB_SHAPE_NONE, "anything else is refused");

    // (2) Each shape sees the delivered message.
    int32_t n = 100;
    deliver(by_ref, ++n, "ref");
    expect(g_seen == n, "const M& callback");
    deliver(by_value, ++n, "value");
    expect(g_seen == n, "by-value callback");
    deliver(generic, ++n, "generic");
    expect(g_seen == n, "generic callback");
    deliver(unique, ++n, "unique");
    expect(g_seen == n, "M::UniquePtr callback");
    deliver(shared, ++n, "shared");
    expect(g_seen == n, "M::SharedPtr callback");
    deliver(shared_cref, ++n, "shared_cref");
    expect(g_seen == n, "const M::SharedPtr& callback");
    deliver(const_shared, ++n, "const_shared");
    expect(g_seen == n, "M::ConstSharedPtr callback");
    deliver(std_const_shared, ++n, "std_const");
    expect(g_seen == n, "std::shared_ptr<const M> callback");

    // A capturing lambda (the jazzy minimal_subscriber shape) and a std::bind
    // over a member taking ConstSharedPtr (multithreaded_executor's shape).
    int32_t captured_seen = -1;
    auto capturing = [&captured_seen](Msg::UniquePtr m) { captured_seen = m->value; };
    static_assert(shape_of<decltype(capturing)>() == d::SUB_SHAPE_UNIQUE_PTR, "capturing");
    deliver(capturing, ++n, "capturing");
    expect(captured_seen == n, "capturing UniquePtr callback");

    Node node;
    auto bound = std::bind(&Node::member_cb, &node, std::placeholders::_1);
    static_assert(shape_of<decltype(bound)>() == d::SUB_SHAPE_CONST_SHARED_PTR,
                  "std::bind over a ConstSharedPtr member");

    // (3) Retention: the kept pointer outlives the frame and is its own copy.
    deliver(bound, 4242, "kept");
    expect(g_seen == 4242, "bound member callback");
    expect(node.last_ != nullptr, "the member kept its pointer");
    expect(node.last_->value == 4242, "the kept message survives the frame's death");
    expect(::std::strcmp(node.last_->text, "kept") == 0, "the kept message is intact");
    expect(node.last_.use_count() == 1, "the node is the only owner after the call");
    Msg::ConstSharedPtr first = node.last_;
    deliver(std::bind(&Node::member_cb, &node, std::placeholders::_1), 7, "next");
    expect(first->value == 4242 && node.last_->value == 7,
           "each delivery is a fresh copy, not one shared buffer");

    ::std::printf("subscription_pointer_callback_runtime: ok\n");
    return 0;
}
