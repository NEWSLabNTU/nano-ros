// phase-476 W2 — a C++ timer's callable lives in the executor arena, and the
// handle `create_wall_timer` returns is two words.
//
// What this RUNS, on the stub RMW backend (timers never reach the RMW, but an
// executor needs a session):
//
//   1. a capturing lambda fires, reading its capture from the ARENA's copy —
//      the caller's lambda object is gone by then;
//   2. a capture that OWNS something is destroyed exactly once: not when the
//      registering call returns (the arena copy still holds it), and once when
//      `timer_.reset()` releases the timer;
//   3. `timer_->cancel()` stops it and `timer_->reset()` restarts it — the two
//      `reset` spellings stay distinct;
//   4. a copy of a released handle is stale and safe: its operations fail and
//      do not reach the timer that took the slot;
//   5. destroying the node releases its timers, and their captures with them;
//   6. the same for a SUBSCRIPTION's capture — before W2 the registering call
//      destroyed its own copy right after the byte copy, while the arena kept
//      dispatching through those bytes, so a capture that owned anything was
//      destroyed while still in use.
//   7. (issue 1667) `sub_.reset()`, `srv_.reset()` and `cli_.reset()` release
//      their registration: the entity leaves the backend, a subscription's
//      capture is destroyed, and a stale copy reaches nothing — not even the
//      registration of a DIFFERENT kind that took its slot;
//   8. (issue 1667) a timer that calls `timer_.reset()` on ITSELF from its own
//      callback (the upstream one-shot idiom) fires once, and its capture
//      survives until the callback returns.
//   9. (issue 1667) a destroyed `GuardCondition` gives its callback slot back:
//      100 create/destroy cycles, far past `NROS_EXECUTOR_MAX_CBS`, all succeed.

#include "nros/executor.hpp"
#include "nros/nros.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <chrono>
#include <cstdio>

extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

namespace {

int g_failures = 0;

void check(bool ok, const char* what) {
    if (!ok) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s\n", what);
    }
}

/// Counts its own destructions. Copies and moves are counted as live objects,
/// so "destroyed exactly once" means: the number of live `Tracked` objects
/// returns to where it started, and never goes below it.
struct Tracked {
    static int live;
    int* fires;
    explicit Tracked(int* f) : fires(f) { ++live; }
    Tracked(const Tracked& o) : fires(o.fires) { ++live; }
    Tracked(Tracked&& o) : fires(o.fires) { ++live; }
    ~Tracked() { --live; }
};
int Tracked::live = 0;

/// A message type the stub backend can carry: nothing is ever received, the
/// subscription only has to register.
struct CounterMsg {
    int32_t data = 0;
    static const size_t SERIALIZED_SIZE_MAX = 16;
    static constexpr const char* TYPE_NAME = "std_msgs::msg::dds_::Int32_";
    static constexpr const char* TYPE_HASH = "RIHS01_int32_stub";
    static int ffi_publish(void*, const void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
        if (out) *out = 0;
        return 0;
    }
    static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
};

// Mirror of a codegen'd service binding (cf. example_interfaces/srv/AddTwoInts).
struct AddTwoInts {
    struct Request {
        int64_t a = 0;
        static const size_t SERIALIZED_SIZE_MAX = 32;
        static constexpr const char* TYPE_HASH = "RIHS01_add_two_ints_request_stub";
        static constexpr const char* TYPE_NAME =
            "example_interfaces::srv::dds_::AddTwoInts_Request_";
        static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
            if (out) *out = 0;
            return 0;
        }
        static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    };
    struct Response {
        int64_t sum = 0;
        static const size_t SERIALIZED_SIZE_MAX = 32;
        static constexpr const char* TYPE_HASH = "RIHS01_add_two_ints_response_stub";
        static constexpr const char* TYPE_NAME =
            "example_interfaces::srv::dds_::AddTwoInts_Response_";
        static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
            if (out) *out = 0;
            return 0;
        }
        static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    };
    static constexpr const char* TYPE_NAME = "example_interfaces::srv::dds_::AddTwoInts_";
};

void on_guard(void*) {}

void on_request(const AddTwoInts::Request&, AddTwoInts::Response&) {}
void on_response(const AddTwoInts::Response&) {}

/// Spin until `pred` holds, or give up after ~1 s of 10 ms spins.
template <typename P> bool spin_until(nros::Executor& exec, P pred) {
    for (int i = 0; i < 100; ++i) {
        (void)exec.spin_once(10);
        if (pred()) return true;
    }
    return false;
}

} // namespace

int main() {
    // A subscription needs the backend to accept entities; timers never reach it.
    nros_stub_rmw_set_accept_entities(true);
    nros::Executor exec;
    check(nros::Executor::create_with_rmw(exec, NROS_STUB_RMW_NAME, nullptr, 0, "w2_timer").ok(),
          "executor create on the stub backend");

    int fires = 0;
    const int live_before = Tracked::live;
    {
        rclcpp::Node node;
        check(exec.create_node(node, "w2_node").ok(), "create the node");

        rclcpp::Timer::SharedPtr timer;
        {
            Tracked owner(&fires);
            timer =
                node.create_wall_timer(std::chrono::milliseconds(1), [owner]() { ++*owner.fires; });
        }
        // (2) the lambda the caller built is gone; the arena copy is alive.
        check(Tracked::live == live_before + 1,
              "after registration exactly ONE capture is alive: the arena's copy");

        // (1) it fires, through the arena's copy.
        check(static_cast<bool>(timer),
              "a successful create_wall_timer returns a non-empty handle");
        check(spin_until(exec, [&]() { return fires > 0; }), "the capturing timer fires");

        // (3) the two reset spellings.
        check(timer->cancel().ok(), "timer_->cancel() succeeds");
        check(timer->is_canceled(), "a cancelled timer reports is_canceled()");
        const int after_cancel = fires;
        for (int i = 0; i < 10; ++i)
            (void)exec.spin_once(10);
        check(fires == after_cancel, "a cancelled timer does not fire");
        check(timer->reset().ok(), "timer_->reset() restarts the timer");
        check(spin_until(exec, [&]() { return fires > after_cancel; }),
              "a restarted timer fires again");

        // (4) a copy, then a release through the original.
        rclcpp::Timer::SharedPtr copy = timer;
        timer.reset();
        check(!timer, "timer_.reset() empties the handle");
        check(Tracked::live == live_before, "timer_.reset() destroyed the capture, exactly once");
        // Another timer takes the released slot.
        int other_fires = 0;
        auto other = node.create_wall_timer(std::chrono::milliseconds(1),
                                            [&other_fires]() { ++other_fires; });
        check(!copy->cancel().ok(), "an operation through a stale copy fails");
        check(spin_until(exec, [&]() { return other_fires > 0; }),
              "the stale copy did not cancel the timer that took its slot");
        copy.reset(); // releasing through a stale copy is a no-op, not a crash
        check(static_cast<bool>(other) && !other->is_canceled(),
              "releasing through a stale copy left the new timer running");

        // (7) the dispatch handles release on `.reset()` (issue 1667).
        {
            const int32_t entities = nros_stub_rmw_live_entities();
            const int live_mark = Tracked::live;
            rclcpp::Subscription<CounterMsg>::SharedPtr sub;
            {
                Tracked sub_owner(&fires);
                sub = node.create_subscription<CounterMsg>(
                    "w2_release", 1, [sub_owner](const CounterMsg&) { ++*sub_owner.fires; });
            }
            check(static_cast<bool>(sub), "create_subscription returns a non-empty handle");
            check(nros_stub_rmw_live_entities() == entities + 1, "the subscriber is live");
            check(Tracked::live == live_mark + 1, "the subscription's capture is alive");
            rclcpp::Subscription<CounterMsg>::SharedPtr sub_copy = sub;
            sub.reset();
            check(!sub, "sub_.reset() empties the handle");
            check(nros_stub_rmw_live_entities() == entities,
                  "sub_.reset() took the subscriber out of the backend");
            check(Tracked::live == live_mark, "sub_.reset() destroyed the capture, exactly once");
            // A TIMER takes the released slot; resetting the stale subscription
            // copy must not release it.
            int slot_fires = 0;
            auto slot_taker = node.create_wall_timer(std::chrono::milliseconds(1),
                                                     [&slot_fires]() { ++slot_fires; });
            sub_copy.reset();
            check(spin_until(exec, [&]() { return slot_fires > 0; }),
                  "a stale subscription copy's reset() did not release the timer in its slot");
            slot_taker.reset();

            rclcpp::Service<AddTwoInts>::SharedPtr srv =
                node.create_service<AddTwoInts>("/w2_add", &on_request);
            rclcpp::Client<AddTwoInts>::SharedPtr cli =
                node.create_client<AddTwoInts>("/w2_add", &on_response);
            check(static_cast<bool>(srv) && static_cast<bool>(cli),
                  "create_service / create_client return non-empty handles");
            check(nros_stub_rmw_live_entities() == entities + 2, "the server and client are live");
            rclcpp::Client<AddTwoInts>::SharedPtr cli_copy = cli;
            srv.reset();
            check(nros_stub_rmw_live_entities() == entities + 1,
                  "srv_.reset() took the server out of the backend");
            cli.reset();
            check(nros_stub_rmw_live_entities() == entities,
                  "cli_.reset() took the client out of the backend");
            AddTwoInts::Request req;
            check(!cli_copy.async_send_request(req).ok(),
                  "a send through a stale client copy fails");
            cli_copy.reset();
            check(nros_stub_rmw_live_entities() == entities,
                  "resetting a stale client copy releases nothing");
        }

        // (8) a one-shot timer that releases itself from inside its callback.
        {
            const int live_mark = Tracked::live;
            int one_shot_fires = 0;
            bool capture_alive_after_reset = false;
            rclcpp::Timer::SharedPtr one_shot;
            {
                Tracked shot_owner(&one_shot_fires);
                one_shot = node.create_wall_timer(
                    std::chrono::milliseconds(1),
                    [&one_shot, &capture_alive_after_reset, live_mark, shot_owner]() {
                        one_shot.reset();
                        // The capture (shot_owner) is still ours until we return.
                        capture_alive_after_reset = Tracked::live == live_mark + 1;
                        ++*shot_owner.fires;
                    });
            }
            check(spin_until(exec, [&]() { return one_shot_fires > 0; }), "the one-shot fires");
            for (int i = 0; i < 10; ++i)
                (void)exec.spin_once(10);
            check(one_shot_fires == 1, "a timer that reset itself fires exactly once");
            check(capture_alive_after_reset,
                  "the capture survived timer_.reset() until the callback returned");
            check(Tracked::live == live_mark, "and was destroyed once the callback returned");
        }

        // (9) a guard condition's destructor releases its entry.
        {
            bool all_created = true;
            for (int i = 0; i < 100 && all_created; ++i) {
                nros::GuardCondition guard;
                all_created = node.create_guard_condition(guard, &on_guard).ok();
                if (all_created) {
                    all_created = guard.trigger().ok();
                }
                (void)exec.spin_once(0);
            }
            check(all_created, "100 guard create/destroy cycles never ran out of callback slots");
        }

        // (5) node destruction releases the remaining timer, and a capture with it.
        Tracked owner2(&fires);
        auto last =
            node.create_wall_timer(std::chrono::milliseconds(1), [owner2]() { ++*owner2.fires; });
        (void)last;
        check(Tracked::live == live_before + 2, "owner2 plus the arena's copy of it");

        // (6) a subscription's capture: alive once registered, not before.
        {
            Tracked sub_owner(&fires);
            auto sub = node.create_subscription<CounterMsg>(
                "w2_capture", 1, [sub_owner](const CounterMsg&) { ++*sub_owner.fires; });
            (void)sub;
        }
        check(Tracked::live == live_before + 3,
              "a subscription's arena copy is alive after its caller's lambda is gone "
              "(owner2, its timer copy, and the subscription copy; sub_owner is gone)");
    }
    check(Tracked::live == live_before,
          "destroying the node released its timers and destroyed their captures");

    if (g_failures != 0) {
        std::fprintf(stderr, "arena_capture_lifetime_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("arena_capture_lifetime_runtime: OK\n");
    return 0;
}
