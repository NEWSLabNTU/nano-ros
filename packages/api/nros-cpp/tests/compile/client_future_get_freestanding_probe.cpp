/*
 * NEGATIVE probe — issue 1996, `rclcpp::Future::get()` WITHOUT the hosted
 * surface.
 *
 * Upstream's `get()` returns an owning `std::shared_ptr<Response>`. A hosted
 * build (`-DNROS_CPP_STD`) has it (`client_future_shape_runtime.cpp`); a
 * freestanding build has neither `<memory>` nor an allocator, so the name is a
 * `static_assert` naming the spelling that works there, `try_take`.
 *
 * `just check cpp` compiles this WITHOUT `NROS_CPP_STD` and requires it to FAIL
 * with `REFUSED by nano-ros` AND `try_take` in the text: an include typo also
 * fails to compile, and the grep is what tells the refusal from that.
 */

#include <rclcpp/rclcpp.hpp>

namespace {

struct FakeResponse {
    static const size_t SERIALIZED_SIZE_MAX = 16;
    int sum;
};

} // namespace

int client_future_get_freestanding_probe();
int client_future_get_freestanding_probe() {
    rclcpp::Future<FakeResponse> fut;
    // The line the stock minimal_client writes after spin_until_future_complete.
    fut.get();
    return 0;
}
