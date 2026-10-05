// Compile regression for issue 0796: the C++ CALLBACK tier's accepted-goal
// hook and client-side cancel.
//
// Both were missing while C and Rust had them: C takes
// `nros_accepted_callback_t` at `nros_action_server_init` and has
// `nros_action_cancel_goal`; Rust takes an accepted callback in
// `create_action_server_with_callbacks(goal, cancel, accepted)` and has
// `ActionClient::cancel_goal`. `nros::ActionServer<A>` had only
// `set_goal_callback` / `set_cancel_callback`, so a user who returned
// ACCEPT_AND_DEFER was never told the goal had been accepted, and
// `nros::ActionClient<A>` had no cancel at all.
//
// The header `-fsyntax-only` loop in `just check cpp` only PARSES these
// templates. This TU INSTANTIATES them against an action type matching the
// generated shape, so `set_accepted_callback`, `install_callbacks`,
// `cancel_goal` and `try_recv_cancel_response` are type-checked — including
// the trampoline whose address is taken.
//
// `just check cpp` compiles this with `-fsyntax-only -std=c++14`.
#include <nros/nros.hpp>

#include <type_traits>

namespace nros_cpp_action_callback_tier_compile_test {

// Mirror of a codegen'd message (the three action payloads share the shape).
struct Payload {
    int32_t value{0};
    static const size_t SERIALIZED_SIZE_MAX = 32;
    static constexpr const char* TYPE_NAME = "test_msgs::action::dds_::Fib_";
    static constexpr const char* TYPE_HASH = "RIHS01_fib_stub";
    static int ffi_deserialize(const uint8_t*, size_t, Payload*) { return 0; }
    static int ffi_serialize(const Payload*, uint8_t*, size_t, size_t* out) {
        if (out) *out = 0;
        return 0;
    }
};

// Mirror of a codegen'd action type.
struct Fib {
    using Goal = Payload;
    using Result = Payload;
    using Feedback = Payload;
    static constexpr const char* TYPE_NAME = "test_msgs::action::dds_::Fib_";
};

struct UserState {
    int accepted_count{0};
};

inline ::nros::Result instantiate_server(::rclcpp::Node& node) {
    ::nros::ActionServer<Fib> server;
    ::nros::Result r = node.create_action_server(server, "/fib");

    // Deferring is exactly the case that needs the accepted hook: the goal
    // callback must return promptly, so the work starts in the hook.
    (void)server.set_goal_callback(
        [](const uint8_t[16], const Fib::Goal&) { return ::nros::GoalResponse::ACCEPT_AND_DEFER; });
    (void)server.set_cancel_callback(
        [](const uint8_t[16]) { return ::nros::CancelResponse::ACCEPT; });
    (void)server.set_accepted_callback([](const uint8_t[16]) {});

    static UserState state;
    (void)server.set_accepted_callback_with_ctx(
        [](const uint8_t[16], void* ctx) { static_cast<UserState*>(ctx)->accepted_count++; },
        &state);

    // Terminating a deferred goal in a NON-success state is the sibling fix
    // (issue 0796 problem 2); keep both instantiated together.
    Fib::Result result;
    (void)server.complete_goal(nullptr, ::nros::GoalStatus::Aborted, result);

    // phase-456 W3b — `for_each_active_goal` had ZERO instantiations in the
    // whole tree (measured: no `examples/`, `book/`, test or probe site), so
    // this member template, its inner trampoline and the `reinterpret_cast` on
    // that trampoline's address had never been type-checked anywhere. An
    // uninstantiated member template is not coverage, for the same reason a
    // `required-features` target no recipe enables is not coverage. It is also
    // the member whose visitor W3b moved into the FFI's own `void* ctx`, so the
    // carrier is on this line too.
    (void)server.for_each_active_goal([](const uint8_t[16], ::nros::GoalStatus) {});
    return r;
}

inline ::nros::Result instantiate_client(::rclcpp::Node& node) {
    ::nros::ActionClient<Fib> client;
    ::nros::Result r = node.create_action_client(client, "/fib");

    uint8_t goal_id[16] = {0};
    (void)client.cancel_goal(goal_id);

    // The reply is the CancelGoal RPC RETURN CODE, not the per-goal decision.
    ::nros::CancelReturnCode code = ::nros::CancelReturnCode::Ok;
    (void)client.try_recv_cancel_response(code);
    return r;
}

// The two cancel enums are DIFFERENT types (issue 0796). They were one name in
// Rust, and their discriminants overlap with opposite meanings — `Reject` and
// `Ok` are both 0 — so a cast between them is never a no-op. Distinct enum
// classes are what makes that a compile error rather than a silent inversion.
static_assert(!std::is_same<::nros::CancelResponse, ::nros::CancelReturnCode>::value,
              "the per-goal cancel decision and the CancelGoal RPC return code must stay "
              "distinct types");
static_assert(::nros::to_ffi(::nros::CancelResponse::REJECT) ==
                  static_cast<int>(::nros::CancelReturnCode::Ok),
              "same FFI byte, opposite verdicts — the reason they cannot share a type");

// Issue 1637 — the per-goal decisions carry rclcpp_action's values
// (`server.hpp`: `GoalResponse : int8_t { REJECT = 1, ACCEPT_AND_EXECUTE = 2,
// ACCEPT_AND_DEFER = 3 }`, `CancelResponse : int8_t { REJECT = 1, ACCEPT = 2 }`),
// reachable under upstream's namespace, and the 0-based FFI discriminant is
// reached ONLY through `to_ffi` (a zero-initialised FFI slot means reject):
// 0/1/2 are `NROS_C_GOAL_{REJECT,ACCEPT_AND_EXECUTE,ACCEPT_AND_DEFER}` and 0/1
// `NROS_C_CANCEL_{REJECT,ACCEPT}` in nros-c's `component.h`.
static_assert(std::is_same<std::underlying_type<::nros::GoalResponse>::type, int8_t>::value,
              "GoalResponse is int8_t, as upstream's is");
static_assert(std::is_same<std::underlying_type<::nros::CancelResponse>::type, int8_t>::value,
              "CancelResponse is int8_t, as upstream's is");
static_assert(static_cast<int>(::rclcpp_action::GoalResponse::REJECT) == 1 &&
                  static_cast<int>(::rclcpp_action::GoalResponse::ACCEPT_AND_EXECUTE) == 2 &&
                  static_cast<int>(::rclcpp_action::GoalResponse::ACCEPT_AND_DEFER) == 3,
              "GoalResponse values are rclcpp_action's");
static_assert(static_cast<int>(::rclcpp_action::CancelResponse::REJECT) == 1 &&
                  static_cast<int>(::rclcpp_action::CancelResponse::ACCEPT) == 2,
              "CancelResponse values are rclcpp_action's");
static_assert(::nros::to_ffi(::nros::GoalResponse::REJECT) == 0 &&
                  ::nros::to_ffi(::nros::GoalResponse::ACCEPT_AND_EXECUTE) == 1 &&
                  ::nros::to_ffi(::nros::GoalResponse::ACCEPT_AND_DEFER) == 2 &&
                  ::nros::to_ffi(static_cast<::nros::GoalResponse>(0)) == 0,
              "to_ffi maps upstream's goal values onto the 0-based FFI; 0 is reject");
static_assert(::nros::to_ffi(::nros::CancelResponse::REJECT) == 0 &&
                  ::nros::to_ffi(::nros::CancelResponse::ACCEPT) == 1 &&
                  ::nros::to_ffi(static_cast<::nros::CancelResponse>(0)) == 0,
              "to_ffi maps upstream's cancel values onto the 0-based FFI; 0 is reject");

// API-shape assertions: a regression in these signatures stops compiling here.
static_assert(
    std::is_same<decltype(std::declval<::nros::ActionClient<Fib>&>().cancel_goal(nullptr)),
                 ::nros::Result>::value,
    "ActionClient<A>::cancel_goal must return nros::Result");

} // namespace nros_cpp_action_callback_tier_compile_test
