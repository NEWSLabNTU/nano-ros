// phase-456 W4 — `nros::Owned<Publisher<M>>` supports the ported publisher
// member pattern, proven BEFORE W5 flips `Publisher<M>::SharedPtr` to it.
//
// W4 decided the open question the phase doc raised: a publisher does NOT get
// an executor-arena slot. The arena holds only what the executor dispatches to
// (`EntryKind` has no publisher) and it has no removal path, so an arena
// publisher could never be destroyed before its executor -- `~Publisher()` and
// `reset()` would become no-ops over a live RMW publisher. The Rust API agrees
// by construction: `Node::create_publisher_with_qos` returns an
// `EmbeddedPublisher<M>` BY VALUE, and `Owned<T>` mirrors that.
//
// What this TU asserts is the consequence for W5. Every operation a ported node
// body performs on its publisher member is exercised against `Owned<T>`:
//
//     rclcpp::Publisher<M>::SharedPtr publisher_;      // default / = nullptr
//     publisher_ = this->create_publisher<M>(...);     // move-assign
//     publisher_->publish(message);                    // operator->
//     if (publisher_) ...                              // explicit operator bool
//     publisher_.reset();                              // drop it now
//
// So when W5 changes the alias, it is flipping to a shape that already
// compiles rather than discovering whether it does. The alias itself is NOT
// touched here -- `ros2_api_adoption.cpp` still asserts the `std::shared_ptr`
// spelling, and inverting it is W5's commit, not this one.
//
// `just check cpp` compiles this with `-fsyntax-only -std=c++14`; the sweep
// also runs it at `-std=c++17` and with `NROS_CPP_STD`.
#include <nros/nros.hpp>
#include <nros/owned.hpp>
#include <nros/publisher.hpp>

namespace nros_cpp_owned_publisher_ported_shape_compile_test {

// Mirror of a codegen'd message (cf. std_msgs/msg/Int32).
struct Int32 {
    int32_t data{0};
    static const size_t SERIALIZED_SIZE_MAX = 16;
    static constexpr const char* TYPE_NAME = "std_msgs::msg::dds_::Int32_";
    static constexpr const char* TYPE_HASH = "RIHS01_int32_stub";
    static int ffi_publish(void*, const void*) { return 0; }
    static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
        if (out) *out = 0;
        return 0;
    }
};

/// What `Publisher<M>::SharedPtr` becomes in W5.
using PublisherOwned = ::nros::Owned<::nros::Publisher<Int32>>;

// A publisher holds no state derived from `M`, which is the measured reason the
// arena argument does not reach it: 872 bytes for every message type, being the
// 608-byte runtime handle plus the 256-byte topic-name cache plus a flag. If a
// transmit buffer ever appears in here, the assertion below is the first thing
// to fail, and the decision above is the thing to re-open.
struct BigPayload {
    uint8_t blob[65536];
    static const size_t SERIALIZED_SIZE_MAX = 65552;
    static constexpr const char* TYPE_NAME = "sensor_msgs::msg::dds_::Image_";
    static constexpr const char* TYPE_HASH = "RIHS01_image_stub";
    static int ffi_publish(void*, const void*) { return 0; }
    static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
};

static_assert(sizeof(::nros::Publisher<Int32>) == sizeof(::nros::Publisher<BigPayload>),
              "phase-456 W4 -- a publisher carries nothing sized from the message type, which "
              "is why moving it into the arena would relocate its bytes rather than remove "
              "them. A message 8000x larger must not make the publisher one byte bigger.");

// `Owned<T>` adds one flag word to the entity it holds, and nothing else. No
// control block, no allocation -- which is the whole reason it exists where
// `std::shared_ptr` cannot go.
static_assert(sizeof(PublisherOwned) <= sizeof(::nros::Publisher<Int32>) + sizeof(void*),
              "nros::Owned<T> must cost at most one word over T");

/// The ported node body, written the way the upstream tutorial writes it.
class PortedTalker {
  public:
    explicit PortedTalker(::rclcpp::Node& node) {
        ::nros::Publisher<Int32> created;
        if (node.create_publisher<Int32>(created, "topic").ok()) {
            // W5's `publisher_ = this->create_publisher<Int32>("topic", 10);`
            publisher_ = PublisherOwned(::nros::tr::forward_rvalue(created));
        }
    }

    void tick() {
        Int32 message;
        message.data = 42;
        // `if (publisher_)` then `publisher_->publish(message)` -- the two
        // operations the corpus performs 41 times between them. `operator->`
        // is what a two-word arena handle deliberately does not have, and
        // what makes this shape work.
        if (publisher_) (void)publisher_->publish(message);
    }

    /// The rest of the publisher surface the corpus reaches: `publish_raw`
    /// (2 sites), `is_valid` (1) and `assert_liveliness` (1). `loan` is the
    /// fourth and is exercised through the same `operator->`.
    void other_verbs() {
        if (!publisher_) return;
        const uint8_t cdr[4] = {0, 1, 0, 0};
        (void)publisher_->publish_raw(cdr, sizeof(cdr));
        (void)publisher_->is_valid();
        (void)publisher_->assert_liveliness();
        (void)publisher_->get_topic_name();
    }

    /// `pub_.reset()` in a ported body means "I am done with this". For a
    /// publisher that DESTROYS, because there is exactly one reference -- which
    /// is the behaviour an arena slot could not have provided, since the arena
    /// has no removal path.
    void release() { publisher_.reset(); }

    bool held() const { return static_cast<bool>(publisher_); }

  private:
    PublisherOwned publisher_ = nullptr;
};

/// The const view exists without a `ConstSharedPtr` TYPE: a `const Owned<T>&`
/// yields `const T*`. phase-456 W4 refuses `Owned<const T>` with a
/// `static_assert` naming this resolution, because that spelling declares
/// cleanly and is ill-formed on its first move or `reset()`.
inline const char* const_view(const PublisherOwned& p) {
    const ::nros::Publisher<Int32>* view = p.get();
    return view ? view->get_topic_name() : "";
}

/// Move-construction and move-assignment, which a member survives a node move
/// by. The RMW handle relocates (`nros_cpp_publisher_relocate`), so this is a
/// supported operation and not merely a compiling one.
inline PublisherOwned relocate(PublisherOwned src) {
    PublisherOwned dst;
    dst = ::nros::tr::forward_rvalue(src);
    return dst;
}

/// Force the bodies above to be type-checked.
inline bool instantiate(::rclcpp::Node& node) {
    PortedTalker talker(node);
    talker.tick();
    talker.other_verbs();
    bool was_held = talker.held();
    talker.release();
    PublisherOwned empty;
    (void)const_view(empty);
    (void)relocate(::nros::tr::forward_rvalue(empty));
    return was_held && (empty == nullptr);
}

} // namespace nros_cpp_owned_publisher_ported_shape_compile_test
