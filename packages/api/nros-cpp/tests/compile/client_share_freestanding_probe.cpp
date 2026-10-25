/*
 * NEGATIVE probe — issue 2037, `FutureAndRequestId::share()` WITHOUT the hosted
 * surface.
 *
 * Upstream's `share()` returns a `SharedFutureAndRequestId`, a
 * `std::shared_future` over a `std::promise`. A hosted build has it
 * (`client_future_and_request_id_runtime.cpp`); a freestanding build has no
 * `std::promise` and no heap, so the name is a `static_assert` naming what
 * works there.
 *
 * `just check cpp` compiles this WITHOUT `NROS_CPP_STD` and requires it to FAIL
 * with `REFUSED by nano-ros` AND `issue 2037` in the text: an include typo also
 * fails to compile, and the grep is what tells the refusal from that.
 */

#include <rclcpp/rclcpp.hpp>

namespace {

struct FakeSrv {
    struct Request {
        static const size_t SERIALIZED_SIZE_MAX = 16;
    };
    struct Response {
        static const size_t SERIALIZED_SIZE_MAX = 16;
    };
};

} // namespace

int client_share_freestanding_probe();
int client_share_freestanding_probe() {
    rclcpp::PollClient<FakeSrv>::FutureAndRequestId f;
    (void)f.request_id; // the id is freestanding-safe
    f.share();
    return 0;
}
