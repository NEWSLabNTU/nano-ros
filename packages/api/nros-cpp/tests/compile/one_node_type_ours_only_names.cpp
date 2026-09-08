// phase-427 W4 — the ours-only `create_*` family carries `_in` NAMES, and this
// file is the proof that the rename was load-bearing rather than cosmetic.
//
// THE COLLISION THE MERGE WOULD HAVE MANUFACTURED
//
// `nros::ComponentNode::create_publisher<M>(const char*, const QoS&)` returned a
// publisher BY VALUE and reported failure through an `ok()` latch. Upstream's
// `rclcpp::Node::create_publisher<M>(const std::string&, …)` returns a
// `shared_ptr` and throws. Those are two different types, two different
// lifetimes and two different failure channels — and as OVERLOADS on the one
// merged type they differ only in SIGNATURE, which C++ resolves silently.
//
// So the ours-only family took a different NAME instead. That is the third
// application of the rule in this campaign, after the reordered C node
// initialiser and the clock-taking C timer verb: where a difference would be
// settled silently by overload resolution, change the name, not the signature.
//
// WHAT WAS MEASURED, AND HOW IT CORRECTED THE WORK ITEM
//
// The item predicted that `create_publisher<M>("chatter", 10)` — the integer
// depth — would bind OURS, reasoning that array-to-pointer decay is a standard
// conversion while `std::string` needs a user-defined one. Compiled against the
// real headers (gcc 13, `-std=c++17`), the merged shape actually does this:
//
//   create_publisher<M>("chatter", 10)               -> UPSTREAM (shared_ptr).
//       `10 -> size_t` is a standard conversion and rescues upstream's second
//       parameter, so ours is not better in every argument and does not win.
//
//   create_publisher<M>("chatter", rclcpp::QoS(10))  -> OURS (by value).
//       Here upstream needs a user-defined conversion on the topic AND ours is
//       exact on it, so ours wins the first argument; gcc accepts the call with
//       only a note that ISO calls it ambiguous. THIS is the silent one, and it
//       is the spelling a ported rclcpp file is most likely to carry.
//
// So the collision is real and it is silent, but the trigger is the explicit-QoS
// spelling rather than the integer one. `pre_rename_negative_control` below
// pins that measurement, so the next person reads the behaviour instead of the
// prediction.
//
// WHAT THIS FILE PROVES
//   1. On the REAL merged type, every ported `create_publisher` spelling reaches
//      UPSTREAM's overload — there is no ours-only candidate left to win.
//   2. The ours-only verbs answer only to their `_in` names.
//   3. NEGATIVE CONTROL: the pre-rename shape, reconstructed exactly, binds the
//      WRONG overload. Without this the file would pass just as well if the
//      collision had never existed, which is the failure mode this campaign
//      keeps hitting — a gate whose subject disappeared.

#include <nros/nros.hpp>

#include <cstddef>
#include <cstdint>
#include <memory>
#include <string>
#include <type_traits>

namespace nros_cpp_ours_only_names_test {

struct CounterMsg {
    int32_t data = 0;
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

// --- (1) the ported spellings reach UPSTREAM on the real type ----------------
//
// `decltype` on the call expression, not a comment: this asks the compiler
// which overload it PICKED, which is the only question that matters. A
// `shared_ptr` return means upstream's; anything else means an ours-only
// candidate survived the rename and is stealing ported calls again.

void ported_spellings_bind_upstream(::rclcpp::Node& node) {
    using DepthCall = decltype(node.create_publisher<CounterMsg>(::std::string("chatter"), 10));
    static_assert(::std::is_same<DepthCall, ::std::shared_ptr<::nros::Publisher<CounterMsg>>>::value,
                  "create_publisher(topic, depth) must reach UPSTREAM's shared_ptr overload");

    // The spelling that USED to bind ours — a string literal plus an explicit
    // QoS. This is the regression test proper: before the rename this exact
    // line returned `Publisher<CounterMsg>` by value.
    using QosCall = decltype(node.create_publisher<CounterMsg>("chatter", ::rclcpp::QoS(10)));
    static_assert(::std::is_same<QosCall, ::std::shared_ptr<::nros::Publisher<CounterMsg>>>::value,
                  "create_publisher(\"literal\", QoS) must reach UPSTREAM's shared_ptr overload -- "
                  "if this fires, an ours-only create_publisher is back on the type and a ported "
                  "file is silently getting a different type, lifetime and failure channel");

    // And the ours-only verb still exists, under its own name, returning by
    // value. `_in` is REACHABLE — the rename must not have deleted the
    // capability, only moved it off the colliding spelling.
    using OursCall = decltype(node.create_publisher_in<CounterMsg>("chatter"));
    static_assert(::std::is_same<OursCall, ::nros::Publisher<CounterMsg>>::value,
                  "create_publisher_in<M>(topic) must return the publisher BY VALUE");
}

// --- (2) the ours-only verbs answer only to `_in` ----------------------------
//
// A pool-bearing node, which is where the storage-free timer verbs live now.
// `NodeWithTimers<N>` IS-A `Node`, so this also pins that the merge kept the
// IS-A relationship `ComponentNode` never had.

class PooledNode : public ::nros::NodeWithTimers<2> {
  public:
    explicit PooledNode(::nros::NodeHandle h) : ::nros::NodeWithTimers<2>(h, "pooled") {
        pub_ = create_publisher_in<CounterMsg>("/chatter");
        create_wall_timer_in<PooledNode, &PooledNode::on_tick>(500);
        create_subscription_in<CounterMsg, PooledNode, &PooledNode::on_msg>("/chatter");

        ::nros::CallbackGroup grp = create_callback_group("ctrl");
        create_timer_in<PooledNode, &PooledNode::on_tick>(grp, 10);
        create_subscription_in<CounterMsg, PooledNode, &PooledNode::on_msg>(grp, "/grouped");

        // The ERGONOMIC macros, instantiated. They moved from
        // `component_node.hpp` to `component.hpp` and their expansions changed
        // verb (`create_subscription` -> `create_subscription_in`,
        // `create_wall_timer` -> `create_wall_timer_in`), and a macro whose
        // expansion no longer names a member is a compile error only at a CALL
        // SITE — which is precisely how the `bind_timer` retirement rotted:
        // header-parse lanes never instantiate a template body.
        NROS_SUBSCRIBE(CounterMsg, on_msg, "/macro");
        NROS_SUBSCRIBE(CounterMsg, on_msg, "/macro-qos", ::nros::QoS(10));
        NROS_CREATE_WALL_TIMER(250, on_tick);
    }

    void on_tick() {}
    void on_msg(const CounterMsg&) {}

  private:
    ::nros::Publisher<CounterMsg> pub_;
};

static_assert(::std::is_base_of<::nros::Node, PooledNode>::value,
              "a component IS-A Node now -- ComponentNode only ever wrapped one");

// The pool is opt-in, and that is the whole point of making its depth a
// template parameter: a plain `Node` must not carry the bytes.
static_assert(sizeof(::nros::NodeWithTimers<2>) > sizeof(::nros::Node),
              "NodeWithTimers<N> is the node PLUS a pool");

// --- (3) NEGATIVE CONTROL — the pre-rename shape binds the wrong overload ----
//
// Reconstructs the two families as the merge would have left them on ONE type,
// with the ours-only member under upstream's BARE name. If overload resolution
// had separated them safely, `MergedBeforeRename` would pick the shared_ptr
// form and these assertions would fail — which is exactly what makes this a
// control rather than a restatement.
//
// The stand-in return types are what let the assertion NAME the winner; the
// signatures are copied from the real headers.

template <typename M> struct ValuePub {};
template <typename M> struct SharedPub {};

struct MergedBeforeRename {
    // OURS, under upstream's bare name — the shape W4 refused to ship.
    template <typename M>
    ValuePub<M> create_publisher(const char*, const ::nros::QoS& = ::nros::QoS::default_profile());
    // UPSTREAM.
    template <typename M> SharedPub<M> create_publisher(const ::std::string&, const ::rclcpp::QoS&);
    template <typename M> SharedPub<M> create_publisher(const ::std::string&, ::size_t);
};

void pre_rename_negative_control(MergedBeforeRename& n) {
    // THE DEFECT: a ported call with an explicit QoS silently takes the
    // by-value, latch-reporting overload.
    using Bad = decltype(n.create_publisher<CounterMsg>("chatter", ::rclcpp::QoS(10)));
    static_assert(::std::is_same<Bad, ValuePub<CounterMsg>>::value,
                  "NEGATIVE CONTROL: the pre-rename shape must bind OURS here. If this fires, the "
                  "collision this rename exists to prevent no longer reproduces, and the positive "
                  "assertions above have stopped testing anything -- re-derive them before "
                  "deleting this control.");

    // And the measured correction to the work item's prediction: the INTEGER
    // depth spelling was never the silent one. Pinned so the reasoning in the
    // header comment cannot rot into folklore.
    using Ok = decltype(n.create_publisher<CounterMsg>("chatter", 10));
    static_assert(::std::is_same<Ok, SharedPub<CounterMsg>>::value,
                  "NEGATIVE CONTROL: `(\"chatter\", 10)` binds UPSTREAM even before the rename -- "
                  "`10 -> size_t` is a standard conversion and rescues upstream's second argument");
}

} // namespace nros_cpp_ours_only_names_test
