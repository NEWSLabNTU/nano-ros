/*
 * NEGATIVE probe — `cpp:Client::wait_for_service` on the DISPATCH client.
 *
 * phase-417 stage 3 refused the argument-free `wait_for_service()`, because
 * upstream's default is -1 (WAIT FOREVER), this helper drives the executor
 * cooperatively (RFC-0021), and `uint32_t` has no value to port -1 to. When
 * phase-456 W9 split the client in two, the verb existed only on
 * `nros::PollClient<S>`, so ONE probe covered the whole surface.
 *
 * The 2026-09-28 gap closure put `wait_for_service` back on the dispatch road —
 * `rclcpp::Client<S>` here, `nros::ClientHandle<S>` in the sibling TU — so the
 * refusal has THREE sites and needs three TUs. Same reason phase-417 gave for
 * splitting the service and action halves: an expected-failure compile proves
 * only that a file does not build, so two refusals in one TU cannot be told
 * apart and a half-fix would still read as green.
 *
 * `just check cpp` requires this TU to FAIL with `REFUSED by nano-ros` in the
 * text. The POSITIVE half — the BUDGETED call on this same type — is in
 * `rclcpp_node_freestanding_surface.cpp`, which is compiled first.
 */

#include <nros/nros.hpp>

#include <nros/client.hpp>

namespace {

// Minimal generated-shape service: `wait_for_service` names nothing on `S`, but
// `Client<S>` needs the nested request/response types to be declarable.
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

int ros2_refuse_unbounded_wait_dispatch_probe();
int ros2_refuse_unbounded_wait_dispatch_probe() {
    rclcpp::Client<FakeService> client;
    // The line upstream's own tutorials write.
    (void)client.wait_for_service();
    return 0;
}
