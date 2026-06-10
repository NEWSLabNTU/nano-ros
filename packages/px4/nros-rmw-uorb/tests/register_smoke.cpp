// Phase 115.K.4.0–K.4.3 smoke test.
//
// Stubs both `nros_rmw_cffi_register` (Rust-staticlib side) and the
// uORB ABI (`orb_advertise_multi`, `orb_publish`, …). With both
// stubbed the test driver builds standalone on a dev box without
// PX4 SDK or Rust toolchain.

#include <cstdio>
#include <cstdlib>
#include <cstring>

#include "nros_rmw_uorb.h"
#include "nros_rmw_uorb_registry.h"
#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"

// Pull in the same ABI declarations the backend sees.
#include "uorb_abi.hpp"

namespace {
const nros_rmw_vtable_t* g_stashed_vtable = nullptr;

// uORB ABI mock — tracks call counts + most-recent payload so the
// test can validate the pub/sub trampoline plumbing.
struct MockOrbState {
    int advertise_calls = 0;
    int publish_calls = 0;
    int unadvertise_calls = 0;
    int subscribe_calls = 0;
    int unsubscribe_calls = 0;
    int check_calls = 0;
    int copy_calls = 0;
    uint8_t last_payload[64] = {};
    size_t last_payload_len = 0;
    // Subscriber side — queue a single pending sample.
    bool pending = false;
    uint8_t pending_payload[64] = {};
    size_t pending_len = 0;
    int next_sub_handle = 1;
};
MockOrbState g_orb;
} // namespace

extern "C" nros_rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t* vtable) {
    g_stashed_vtable = vtable;
    return NROS_RMW_RET_OK;
}

extern "C" {

orb_advert_t orb_advertise_multi(const struct orb_metadata* meta, const void* data, int* instance) {
    g_orb.advertise_calls++;
    if (instance != nullptr) {
        *instance = 0;
    }
    if (data != nullptr && meta != nullptr) {
        size_t n = meta->o_size;
        if (n > sizeof(g_orb.last_payload)) {
            n = sizeof(g_orb.last_payload);
        }
        std::memcpy(g_orb.last_payload, data, n);
        g_orb.last_payload_len = n;
    }
    // Non-null sentinel.
    return reinterpret_cast<orb_advert_t>(0xCAFEBABEull);
}

int orb_publish(const struct orb_metadata* meta, orb_advert_t handle, const void* data) {
    g_orb.publish_calls++;
    if (handle == nullptr) {
        return -1;
    }
    if (data != nullptr && meta != nullptr) {
        size_t n = meta->o_size;
        if (n > sizeof(g_orb.last_payload)) {
            n = sizeof(g_orb.last_payload);
        }
        std::memcpy(g_orb.last_payload, data, n);
        g_orb.last_payload_len = n;
    }
    return 0;
}

int orb_unadvertise(orb_advert_t /*handle*/) {
    g_orb.unadvertise_calls++;
    return 0;
}

int orb_subscribe_multi(const struct orb_metadata* /*meta*/, unsigned) {
    g_orb.subscribe_calls++;
    return g_orb.next_sub_handle++;
}
int orb_unsubscribe(int /*handle*/) {
    g_orb.unsubscribe_calls++;
    return 0;
}
int orb_copy(const struct orb_metadata* meta, int /*handle*/, void* buf) {
    g_orb.copy_calls++;
    if (!g_orb.pending || buf == nullptr || meta == nullptr) {
        return -1;
    }
    size_t n = meta->o_size;
    if (n > g_orb.pending_len) n = g_orb.pending_len;
    std::memcpy(buf, g_orb.pending_payload, n);
    g_orb.pending = false;
    return 0;
}
int orb_check(int /*handle*/, bool* updated) {
    g_orb.check_calls++;
    if (updated != nullptr) *updated = g_orb.pending;
    return 0;
}

// Push-wake ABI override. Strong symbols beat the weak default in
// callback_default.cpp at link time. The test driver stashes the
// (cb, arg) pair so it can fire the callback synthetically and
// verify subscriber.cpp's atomic flag latches.
struct PushWakeState {
    nros_orb_callback_t cb = nullptr;
    void* arg = nullptr;
    int last_handle = -1;
    int register_calls = 0;
    int unregister_calls = 0;
} g_push;

int nros_orb_register_callback(const struct orb_metadata* /*meta*/, uint8_t /*instance*/,
                               int handle, nros_orb_callback_t cb, void* arg) {
    g_push.register_calls++;
    g_push.cb = cb;
    g_push.arg = arg;
    g_push.last_handle = handle;
    return 0;
}

int nros_orb_unregister_callback(int handle) {
    g_push.unregister_calls++;
    if (g_push.last_handle == handle) {
        g_push.cb = nullptr;
        g_push.arg = nullptr;
        g_push.last_handle = -1;
    }
    return 0;
}

} // extern "C"

int main() {
    nros_rmw_ret_t rc = nros_rmw_uorb_register();
    if (rc != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "register returned %d, expected 0\n", rc);
        return 1;
    }
    if (g_stashed_vtable == nullptr) {
        std::fprintf(stderr, "vtable pointer not stashed\n");
        return 1;
    }
    // Spot-check: a few slots must be non-null. NULL is reserved for
    // optional event hooks; the lifecycle / data-plane slots must
    // resolve to real (stub-returning-UNSUPPORTED) functions.
    const auto* vt = g_stashed_vtable;
    if (vt->open == nullptr || vt->close == nullptr || vt->create_publisher == nullptr ||
        vt->create_subscriber == nullptr) {
        std::fprintf(stderr, "required vtable slot is NULL\n");
        return 1;
    }
    // K.4.1 — open + close round-trip. uORB ignores the locator,
    // session mode, and domain id; the only validated state is
    // `backend_data` allocated by open and freed by close.
    nros_rmw_session_t session{};
    rc = vt->open("/* ignored */", 0, 0, "test_module", &session);
    if (rc != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "open returned %d, expected OK\n", rc);
        return 1;
    }
    if (session.backend_data == nullptr) {
        std::fprintf(stderr, "open did not populate backend_data\n");
        return 1;
    }
    // drive_io is a no-op for uORB (push-based delivery).
    rc = vt->drive_io(&session, 0);
    if (rc != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "drive_io returned %d, expected OK\n", rc);
        return 1;
    }
    rc = vt->close(&session);
    if (rc != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "close returned %d, expected OK\n", rc);
        return 1;
    }
    if (session.backend_data != nullptr) {
        std::fprintf(stderr, "close did not clear backend_data\n");
        return 1;
    }

    // -- K.4.2 / K.4.3 publisher path --
    //
    // Re-open the session for the publisher exercise (close above
    // freed the state).
    rc = vt->open("", 0, 0, "test_module", &session);
    if (rc != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "re-open returned %d\n", rc);
        return 1;
    }

    // Without a registered topic, create_publisher must reject with
    // TOPIC_NAME_INVALID — distinct from UNSUPPORTED.
    nros_rmw_publisher_t pubp{};
    rc = vt->create_publisher(&session, "/unregistered", "T", "H", 0, nullptr, &pubp);
    if (rc != NROS_RMW_RET_TOPIC_NAME_INVALID) {
        std::fprintf(
            stderr,
            "create_publisher on unregistered topic returned %d, expected TOPIC_NAME_INVALID\n",
            rc);
        return 1;
    }

    // Register a synthetic topic and create the publisher.
    static const struct orb_metadata kFakeMeta = {
        /*o_name           =*/"test_topic",
        /*o_size           =*/8,
        /*o_size_no_padding=*/8,
        /*message_hash      =*/0,
        /*o_id              =*/0,
        /*o_queue           =*/1,
    };
    nros_rmw_uorb_clear_registry();
    rc = nros_rmw_uorb_register_topic("/test_topic", "test::Msg", &kFakeMeta);
    if (rc != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "register_topic returned %d\n", rc);
        return 1;
    }

    rc = vt->create_publisher(&session, "/test_topic", "test::Msg", "H", 0, nullptr, &pubp);
    if (rc != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_publisher returned %d, expected OK\n", rc);
        return 1;
    }
    if (pubp.backend_data == nullptr) {
        std::fprintf(stderr, "create_publisher did not populate backend_data\n");
        return 1;
    }

    // First publish must trigger lazy orb_advertise_multi.
    uint8_t payload[8] = {1, 2, 3, 4, 5, 6, 7, 8};
    rc = vt->publish_raw(&pubp, payload, sizeof(payload));
    if (rc != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "publish_raw[0] returned %d\n", rc);
        return 1;
    }
    if (g_orb.advertise_calls != 1) {
        std::fprintf(stderr, "expected 1 advertise call, got %d\n", g_orb.advertise_calls);
        return 1;
    }
    if (g_orb.publish_calls != 0) {
        std::fprintf(stderr,
                     "first publish must use advertise, not publish; got %d publish calls\n",
                     g_orb.publish_calls);
        return 1;
    }
    if (std::memcmp(g_orb.last_payload, payload, sizeof(payload)) != 0) {
        std::fprintf(stderr, "advertise did not propagate the payload\n");
        return 1;
    }

    // Subsequent publish must use orb_publish.
    payload[0] = 0xff;
    rc = vt->publish_raw(&pubp, payload, sizeof(payload));
    if (rc != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "publish_raw[1] returned %d\n", rc);
        return 1;
    }
    if (g_orb.publish_calls != 1) {
        std::fprintf(stderr, "expected 1 publish call, got %d\n", g_orb.publish_calls);
        return 1;
    }
    if (g_orb.last_payload[0] != 0xff) {
        std::fprintf(stderr, "publish payload mismatch\n");
        return 1;
    }

    // Short payload must reject with BUFFER_TOO_SMALL.
    rc = vt->publish_raw(&pubp, payload, sizeof(payload) - 1);
    if (rc != NROS_RMW_RET_BUFFER_TOO_SMALL) {
        std::fprintf(stderr, "short publish returned %d, expected BUFFER_TOO_SMALL\n", rc);
        return 1;
    }

    // destroy_publisher must unadvertise.
    vt->destroy_publisher(&pubp);
    if (g_orb.unadvertise_calls != 1) {
        std::fprintf(stderr, "expected 1 unadvertise call, got %d\n", g_orb.unadvertise_calls);
        return 1;
    }
    if (pubp.backend_data != nullptr) {
        std::fprintf(stderr, "destroy_publisher did not clear backend_data\n");
        return 1;
    }

    // -- K.4.2 subscriber path --
    //
    // Without a registered topic, create_subscriber must reject
    // with TOPIC_NAME_INVALID.
    nros_rmw_subscriber_t subp{};
    rc = vt->create_subscriber(&session, "/unregistered", "T", "H", 0, nullptr, &subp);
    if (rc != NROS_RMW_RET_TOPIC_NAME_INVALID) {
        std::fprintf(
            stderr,
            "create_subscriber on unregistered topic returned %d, expected TOPIC_NAME_INVALID\n",
            rc);
        return 1;
    }

    rc = vt->create_subscriber(&session, "/test_topic", "test::Msg", "H", 0, nullptr, &subp);
    if (rc != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_subscriber returned %d, expected OK\n", rc);
        return 1;
    }
    if (subp.backend_data == nullptr) {
        std::fprintf(stderr, "create_subscriber did not populate backend_data\n");
        return 1;
    }
    if (g_orb.subscribe_calls != 1) {
        std::fprintf(stderr, "expected 1 subscribe call, got %d\n", g_orb.subscribe_calls);
        return 1;
    }
    // K.4.2-sub-push: subscriber_create must register a callback.
    if (g_push.register_calls != 1 || g_push.cb == nullptr) {
        std::fprintf(stderr, "expected 1 register_callback call with non-null cb, got %d / %p\n",
                     g_push.register_calls, (void*)g_push.cb);
        return 1;
    }

    // First try_recv_raw: the create-time `ready` pin makes us fall
    // through to orb_check even before the broker fires anything;
    // orb_check returns no-update, we re-arm. After re-arm,
    // has_data must short-circuit to 0 *without* an extra orb_check
    // syscall.
    uint8_t rxbuf[16] = {};
    int32_t n = vt->try_recv_raw(&subp, rxbuf, sizeof(rxbuf));
    if (n != NROS_RMW_RET_NO_DATA) {
        std::fprintf(stderr, "try_recv_raw empty[0] returned %d, expected NO_DATA\n", n);
        return 1;
    }
    int check_after_first = g_orb.check_calls;
    // Fast-path check: with `ready` cleared by the re-arm above,
    // has_data must NOT call orb_check.
    if (vt->has_data(&subp) != 0) {
        std::fprintf(stderr, "has_data on empty queue returned non-zero\n");
        return 1;
    }
    if (g_orb.check_calls != check_after_first) {
        std::fprintf(stderr, "has_data fast-path missed: orb_check called %d times, expected %d\n",
                     g_orb.check_calls, check_after_first);
        return 1;
    }
    n = vt->try_recv_raw(&subp, rxbuf, sizeof(rxbuf));
    if (n != NROS_RMW_RET_NO_DATA) {
        std::fprintf(stderr, "try_recv_raw empty[1] returned %d, expected NO_DATA\n", n);
        return 1;
    }
    if (g_orb.check_calls != check_after_first) {
        std::fprintf(stderr,
                     "try_recv_raw fast-path missed: orb_check called %d times, expected %d\n",
                     g_orb.check_calls, check_after_first);
        return 1;
    }

    // Broker fires the callback → ready flips → next poll goes
    // through to orb_check.
    g_push.cb(g_push.arg);

    // Stage a sample and drain.
    g_orb.pending = true;
    g_orb.pending_len = kFakeMeta.o_size;
    for (size_t i = 0; i < kFakeMeta.o_size; ++i) {
        g_orb.pending_payload[i] = static_cast<uint8_t>(0xA0 + i);
    }
    if (vt->has_data(&subp) != 1) {
        std::fprintf(stderr, "has_data with pending returned 0\n");
        return 1;
    }
    n = vt->try_recv_raw(&subp, rxbuf, sizeof(rxbuf));
    if (n != static_cast<int32_t>(kFakeMeta.o_size)) {
        std::fprintf(stderr, "try_recv_raw returned %d, expected %u\n", n, kFakeMeta.o_size);
        return 1;
    }
    for (size_t i = 0; i < kFakeMeta.o_size; ++i) {
        if (rxbuf[i] != static_cast<uint8_t>(0xA0 + i)) {
            std::fprintf(stderr, "rxbuf[%zu] = 0x%02x, expected 0x%02x\n", i, rxbuf[i],
                         0xA0 + (int)i);
            return 1;
        }
    }

    // Short buffer rejects without draining. Re-stage sample +
    // fire callback (push-wake builds need both: the pending flag
    // tells orb_check there's data, and the callback flips
    // SubscriberState::ready so the fast-path doesn't short-
    // circuit).
    g_orb.pending = true;
    g_orb.pending_len = kFakeMeta.o_size;
    g_push.cb(g_push.arg);
    n = vt->try_recv_raw(&subp, rxbuf, /*too small*/ 4);
    if (n != NROS_RMW_RET_BUFFER_TOO_SMALL) {
        std::fprintf(stderr, "short try_recv_raw returned %d, expected BUFFER_TOO_SMALL\n", n);
        return 1;
    }
    if (!g_orb.pending) {
        std::fprintf(stderr, "short try_recv_raw drained the queue (should not)\n");
        return 1;
    }
    // Retry with full buffer drains.
    n = vt->try_recv_raw(&subp, rxbuf, sizeof(rxbuf));
    if (n != static_cast<int32_t>(kFakeMeta.o_size)) {
        std::fprintf(stderr, "retry try_recv_raw returned %d, expected %u\n", n, kFakeMeta.o_size);
        return 1;
    }

    vt->destroy_subscriber(&subp);
    if (g_orb.unsubscribe_calls != 1) {
        std::fprintf(stderr, "expected 1 unsubscribe call, got %d\n", g_orb.unsubscribe_calls);
        return 1;
    }
    if (g_push.unregister_calls != 1) {
        std::fprintf(stderr, "expected 1 unregister_callback call, got %d\n",
                     g_push.unregister_calls);
        return 1;
    }
    if (subp.backend_data != nullptr) {
        std::fprintf(stderr, "destroy_subscriber did not clear backend_data\n");
        return 1;
    }

    vt->close(&session);

    // Null-arg rejection on open.
    rc = vt->open(nullptr, 0, 0, nullptr, nullptr);
    if (rc != NROS_RMW_RET_INVALID_ARGUMENT) {
        std::fprintf(stderr, "open(null out) returned %d, expected INVALID_ARGUMENT\n", rc);
        return 1;
    }

    std::printf("[OK] nros_rmw_uorb K.4.0–K.4.3 (pub + sub) passes\n");
    return 0;
}
