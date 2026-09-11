/*
 * NEGATIVE probe — phase-417 stage 3, the ACTION half of
 * `NROS_RCLCPP_REFUSE_UNBOUNDED_WAIT`.
 *
 * `rclcpp_action::Client<A>::wait_for_action_server(timeout = -1)` waits
 * forever upstream. Ours used to substitute 5000 ms for an argument-free call,
 * exactly as `Client::wait_for_service` did — one defect, two sites, and the
 * sibling is why this file exists: the service probe alone would have passed
 * over a fix that never reached the action client.
 *
 * Its own TU rather than a second call in the service probe: an expected-
 * failure compile proves only that the file does not build, so two refusals in
 * one TU cannot be told apart and a half-fix would still read as green.
 *
 * `just check cpp` requires this TU to FAIL with `REFUSED by nano-ros` in the
 * text. `action_goal_uuid.cpp` is the positive half — it instantiates the same
 * stub action type and must keep compiling.
 */

#include <nros/nros.hpp>

#include <nros/action_client.hpp>

namespace {

// Mirror of a codegen'd message — the same shape `action_goal_uuid.cpp` uses.
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

struct Fib {
    using Goal = Payload;
    using Result = Payload;
    using Feedback = Payload;
    static constexpr const char* TYPE_NAME = "test_msgs::action::dds_::Fib_";
};

} // namespace

int ros2_refuse_unbounded_action_wait_probe();
int ros2_refuse_unbounded_action_wait_probe() {
    nros::ActionClient<Fib> client;
    // The line upstream's own tutorials write.
    (void)client.wait_for_action_server();
    return 0;
}
