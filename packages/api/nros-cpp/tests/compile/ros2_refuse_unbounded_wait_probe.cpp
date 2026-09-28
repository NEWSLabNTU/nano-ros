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
 * This is the FUTURE-STYLE SERVICE half. Every other site of the same refusal
 * is a SEPARATE TU even though they share one message, because an
 * expected-failure compile proves only that the file does not build: with two
 * calls in one TU, a fix that reached only one site would still fail and still
 * read as green. The siblings are
 * `ros2_refuse_unbounded_action_wait_probe.cpp` (the action client),
 * `ros2_refuse_unbounded_wait_dispatch_probe.cpp` (`rclcpp::Client<S>`) and
 * `ros2_refuse_unbounded_wait_handle_probe.cpp` (`nros::ClientHandle<S>`, what
 * `Client<S>::SharedPtr` names) — the last two added 2026-09-28 with the verb
 * itself.
 *
 * `just check cpp` requires this TU to FAIL with `REFUSED by nano-ros` in the
 * text. The POSITIVE half is `service_client_call_polling.cpp` plus the header
 * sweep, which keep the BUDGETED forms compiling.
 */

#include <nros/nros.hpp>

// phase-456 W9 REPOINTED this probe here, because at the time `wait_for_service`
// lived only on the FUTURE-style client — the half that owns the
// `RmwServiceClient` the FFI read — and on `rclcpp::Client<S>` the file would
// have failed with "no member named 'wait_for_service'": non-zero, and NOT the
// refusal, which is the "broke for another reason" the lane's grep exists to
// catch.
//
// 2026-09-28 gave the dispatch road the verb (its FFI takes `(executor,
// handle_id)` now), so that second TU exists and asserts its own refusal. This
// one stays pointed at `nros::PollClient<S>`: the refusal is per DECLARING type,
// and each type's needs its own file.
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
