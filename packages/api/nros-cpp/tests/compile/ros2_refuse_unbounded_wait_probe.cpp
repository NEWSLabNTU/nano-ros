/*
 * NEGATIVE probe — phase-417 stage 3, `cpp:PollClient::wait_for_service` and
 * its sibling `rclcpp_action::Client::wait_for_action_server`.
 *
 * Upstream defaults BOTH timeouts to -1, which means WAIT FOREVER. Ours are
 * `uint32_t` millisecond budgets that used to default to 5000, so a ported
 * argument-free call compiled and returned `Timeout` after five seconds where
 * upstream was still waiting — and `[[nodiscard]]` does not catch
 * `if (!client->wait_for_service())`, which is the shape upstream code writes.
 * RFC-0089: substituting a budget the caller did not choose is the
 * "compiles and differs" the rule forbids.
 *
 * This is the SERVICE half. The action half is a SEPARATE TU
 * (`ros2_refuse_unbounded_action_wait_probe.cpp`) even though the two share one
 * message, because an expected-failure compile proves only that the file does
 * not build: with both calls in one TU, a fix that reached only one site would
 * still fail and still read as green.
 *
 * `just check cpp` requires this TU to FAIL with `REFUSED by nano-ros` in the
 * text. The POSITIVE half is `service_client_call_polling.cpp` plus the header
 * sweep, which keep the BUDGETED forms compiling.
 */

#include <nros/nros.hpp>

// phase-456 W9 — `wait_for_service` lives on the FUTURE-style client, which owns
// the `RmwServiceClient` the FFI reads. `rclcpp::Client<S>` is the dispatch
// client and has no such method at all, so pointing this probe there would make
// it fail on "no member named 'wait_for_service'" — non-zero, and NOT the
// refusal. That is the "broke for another reason" the lane's grep exists to
// catch, and it is why the probe moved rather than being left to pass by
// accident.
#include <nros/polling_client.hpp>

namespace {

// Minimal generated-shape service: `wait_for_service` names nothing on `S`, but
// `PollClient<S>` needs the nested request/response types to be declarable.
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

int ros2_refuse_unbounded_wait_probe();
int ros2_refuse_unbounded_wait_probe() {
    nros::PollClient<FakeService> client;
    // The line upstream's own tutorials write.
    (void)client.wait_for_service();
    return 0;
}
