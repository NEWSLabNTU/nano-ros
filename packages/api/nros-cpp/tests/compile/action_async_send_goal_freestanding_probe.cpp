/*
 * NEGATIVE probe — issue 2015, upstream rclcpp_action's goal-handle shapes on a
 * FREESTANDING build: `Client<A>::async_send_goal`.
 *
 * A hosted build (`-DNROS_CPP_STD`) takes it as a drop-in
 * (`action_upstream_dropins_runtime.cpp`); a freestanding one has no
 * `std::shared_ptr` / allocator for a goal handle, so it is a `static_assert`
 * naming the goal-id-keyed form. `just check cpp` compiles this WITHOUT
 * `NROS_CPP_STD` and requires it to FAIL with `REFUSED by nano-ros` AND
 * `issue 2015` in the text: an include typo also fails to compile, and the grep
 * is what tells the refusal from that.
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

int action_async_send_goal_freestanding_probe(rclcpp_action::Client<Count>& client);
int action_async_send_goal_freestanding_probe(rclcpp_action::Client<Count>& client) {
    Value goal = {1};
    auto fut = client.async_send_goal(goal);
    return fut.valid() ? 0 : 1;
}
