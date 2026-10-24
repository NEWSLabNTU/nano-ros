/*
 * NEGATIVE probe — issue 2027, upstream rclcpp_action's handle-taking
 * `Client<A>::async_cancel_goal` on a FREESTANDING build.
 *
 * A hosted build (`-DNROS_CPP_STD`) returns upstream's `std::shared_future`
 * (`action_goal_handle_api_runtime.cpp`); a freestanding one has neither a
 * goal handle nor a `std::shared_future`, so it is a `static_assert` naming
 * the goal-id-keyed form. `just check cpp` compiles this WITHOUT
 * `NROS_CPP_STD` and requires it to FAIL with `REFUSED by nano-ros` AND
 * `issue 2015` (the goal-handle refusal it shares) in the text.
 */

#include <rclcpp/rclcpp.hpp>
#include <rclcpp_action/rclcpp_action.hpp>

namespace {

struct Value {
    int v;
    static constexpr const char* TYPE_NAME = "probe::action::dds_::Count_";
    static constexpr const char* TYPE_HASH = "RIHS01_stub";
    static const size_t SERIALIZED_SIZE_MAX = 16;
    static int ffi_serialize(const Value*, uint8_t*, size_t, size_t*) { return -1; }
    static int ffi_deserialize(const uint8_t*, size_t, Value*) { return -1; }
};

struct Count {
    using Goal = Value;
    using Result = Value;
    using Feedback = Value;
    static constexpr const char* TYPE_NAME = "probe::action::dds_::Count_";
};

} // namespace

int action_async_cancel_goal_freestanding_probe(rclcpp_action::Client<Count>& client);
int action_async_cancel_goal_freestanding_probe(rclcpp_action::Client<Count>& client) {
    client.async_cancel_goal(nullptr);
    return 0;
}
