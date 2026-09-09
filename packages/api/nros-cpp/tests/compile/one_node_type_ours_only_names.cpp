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
//   4. `_in` and `_in_group` are TWO suffixes, not one. W4's `_in` collided with
//      the one RFC-0047 had used since phase 273 for "in a callback group", so
//      one class carried two families under one name. The rule now is that a
//      creation verb whose FIRST parameter is a callback group ends `_in_group`
//      — and section (4) below asserts both directions, with its own negative
//      control reconstructing the pre-split shape.

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
    static_assert(
        ::std::is_same<DepthCall, ::std::shared_ptr<::nros::Publisher<CounterMsg>>>::value,
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
        create_timer_in_group<PooledNode, &PooledNode::on_tick>(grp, 10);
        create_subscription_in_group<CounterMsg, PooledNode, &PooledNode::on_msg>(grp, "/grouped");

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

// --- (4) `_in` is ours-only; `_in_group` is the callback-group form ----------
//
// phase-427 follow-on. Two suffixes had collapsed into one: `_in` meant "in a
// callback group" from phase 273 (RFC-0047) and, after W4, also "ours-only /
// storage-free". Both meanings sat on `nros::Node`, and one overload —
// `create_subscription_in<M, C, &C::method>(group, topic, qos)` — was in both.
//
// The compiler was never confused: `const CallbackGroup&` and `const char*` do
// not convert to one another, so this was never W4's silent-collision hazard.
// The READER was. The rule that fixes it is mechanical, which is why it can be
// asserted here: a creation verb whose FIRST parameter is a callback group ends
// `_in_group`; `_in` names nothing about scheduling.
//
// Detection, not instantiation: these ask whether a call is WELL-FORMED, which
// is the only way to assert that a spelling is GONE. The `PreSplit*` types are
// the negative controls — each detector must fire on the shape the split
// removed, or it is reporting "absent" for the wrong reason and the assertions
// above it are vacuous.

template <class...> struct make_void_ {
    using type = void;
};
template <class... Ts> using void_t_ = typename make_void_<Ts...>::type;

// Does `create_publisher_in` — the SHORT name — accept a callback group first?
template <class N, class = void> struct pub_short_name_takes_group : ::std::false_type {};
template <class N>
struct pub_short_name_takes_group<
    N, void_t_<decltype(::std::declval<N&>().template create_publisher_in<CounterMsg>(
           ::std::declval<const ::nros::CallbackGroup&>(),
           ::std::declval<::nros::Publisher<CounterMsg>&>(), "t"))>> : ::std::true_type {};

// Does the LONG name exist?
template <class N, class = void> struct pub_has_group_verb : ::std::false_type {};
template <class N>
struct pub_has_group_verb<
    N, void_t_<decltype(::std::declval<N&>().template create_publisher_in_group<CounterMsg>(
           ::std::declval<const ::nros::CallbackGroup&>(),
           ::std::declval<::nros::Publisher<CounterMsg>&>(), "t"))>> : ::std::true_type {};

static_assert(!pub_short_name_takes_group<::nros::Node>::value,
              "`create_publisher_in` must be the OURS-ONLY form only. A callback group as the "
              "first parameter is spelled `create_publisher_in_group` -- one suffix, one meaning "
              "(RFC-0089, \"The `_in` rule, amended\")");
static_assert(pub_has_group_verb<::nros::Node>::value,
              "`create_publisher_in_group` must exist: the split renamed the group form, it did "
              "not delete the capability");

// The timer half, on the type where the two meanings actually overlapped: a
// pool-parked (storage-free) timer that also takes a group.
template <class N, class = void> struct timer_short_name_takes_group : ::std::false_type {};
template <class N>
struct timer_short_name_takes_group<N, void_t_<decltype(::std::declval<N&>().create_timer_in(
                                           ::std::declval<const ::nros::CallbackGroup&>(),
                                           static_cast<uint64_t>(10), nullptr, nullptr))>>
    : ::std::true_type {};

template <class N, class = void> struct timer_has_group_verb : ::std::false_type {};
template <class N>
struct timer_has_group_verb<N, void_t_<decltype(::std::declval<N&>().create_timer_in_group(
                                   ::std::declval<const ::nros::CallbackGroup&>(),
                                   static_cast<uint64_t>(10), nullptr, nullptr))>>
    : ::std::true_type {};

static_assert(!timer_short_name_takes_group<::nros::NodeWithTimers<2>>::value,
              "`create_timer_in` is gone from the pool type -- the pool-parked group form is "
              "`create_timer_in_group`, and it is the overload that made the split necessary "
              "(storage-free AND in a group, so the short name could say neither)");
static_assert(timer_has_group_verb<::nros::NodeWithTimers<2>>::value,
              "`create_timer_in_group` must be reachable on the pool type, including the plain "
              "callback + ctx form");

// NEGATIVE CONTROLS — the pre-split shapes. Each detector must report TRUE on
// the spelling the split removed. If one of these fires, the detector above it
// is answering "absent" for some reason other than the rename (a wrong argument
// list, a changed signature), and its `!` assertion has stopped testing the
// split.

struct PreSplitPublisherNode {
    // The group form, under the SHORT name -- what `nros::Node` carried before.
    template <typename M>
    ::nros::Result create_publisher_in(const ::nros::CallbackGroup&, ::nros::Publisher<M>&,
                                       const char*,
                                       const ::nros::QoS& = ::nros::QoS::default_profile());
    // The ours-only form, which keeps the short name after the split.
    template <typename M>
    ::nros::Publisher<M> create_publisher_in(const char*,
                                             const ::nros::QoS& = ::nros::QoS::default_profile());
};

struct PreSplitTimerNode {
    void create_timer_in(const ::nros::CallbackGroup&, uint64_t, nros_cpp_timer_callback_t,
                         void* = nullptr);
};

static_assert(pub_short_name_takes_group<PreSplitPublisherNode>::value,
              "NEGATIVE CONTROL: on the pre-split shape the SHORT publisher name must accept a "
              "group. If this fires the detector is broken, not the header, and the assertion "
              "that `nros::Node` no longer accepts one is proving nothing");
static_assert(!pub_has_group_verb<PreSplitPublisherNode>::value,
              "NEGATIVE CONTROL: the pre-split shape had no `_in_group` spelling at all");
static_assert(timer_short_name_takes_group<PreSplitTimerNode>::value,
              "NEGATIVE CONTROL: on the pre-split shape the SHORT timer name must accept a group");
static_assert(!timer_has_group_verb<PreSplitTimerNode>::value,
              "NEGATIVE CONTROL: the pre-split shape had no `create_timer_in_group`");

} // namespace nros_cpp_ours_only_names_test
