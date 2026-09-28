/*
 * NEGATIVE probe — `wait_for_service()` with no budget on
 * `nros::ClientHandle<S>`, which is what `rclcpp::Client<S>::SharedPtr` names.
 *
 * The third site of ONE refusal concept (see
 * `ros2_refuse_unbounded_wait_dispatch_probe.cpp` for why each site is its own
 * TU). This is the one a ported file is most likely to hit, because
 * `Client<S>::SharedPtr cli_;` is how ported source declares a client member and
 * `cli_.wait_for_service()` is the line upstream's tutorial writes next.
 *
 * `just check cpp` requires this TU to FAIL with `REFUSED by nano-ros` in the
 * text.
 */

#include <nros/nros.hpp>

#include <nros/client_handle.hpp>

namespace {

struct FakeRequest {
    static const size_t SERIALIZED_SIZE_MAX = 16;
    int a;
};
struct FakeResponse {
    static const size_t SERIALIZED_SIZE_MAX = 16;
    int b;
};
struct FakeService {
    using Request = FakeRequest;
    using Response = FakeResponse;
};

} // namespace

int ros2_refuse_unbounded_wait_handle_probe();
int ros2_refuse_unbounded_wait_handle_probe() {
    nros::ClientHandle<FakeService> cli = nullptr;
    (void)cli.wait_for_service();
    return 0;
}
