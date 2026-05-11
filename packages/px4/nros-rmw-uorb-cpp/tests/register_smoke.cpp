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
const nros_rmw_vtable_t *g_stashed_vtable = nullptr;

// uORB ABI mock — tracks call counts + most-recent payload so the
// test can validate the publisher trampoline plumbing.
struct MockOrbState {
    int advertise_calls = 0;
    int publish_calls = 0;
    int unadvertise_calls = 0;
    uint8_t last_payload[64] = {};
    size_t last_payload_len = 0;
};
MockOrbState g_orb;
} // namespace

extern "C" nros_rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vtable) {
    g_stashed_vtable = vtable;
    return NROS_RMW_RET_OK;
}

extern "C" {

orb_advert_t orb_advertise_multi(const struct orb_metadata *meta,
                                 const void *data, int *instance) {
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

int orb_publish(const struct orb_metadata *meta, orb_advert_t handle,
                const void *data) {
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

// K.4.2 publisher uses only advertise/publish/unadvertise; stub the
// subscriber surface as well so future K.4.2 subscriber work can
// share this test driver.
int orb_subscribe_multi(const struct orb_metadata *, unsigned) {
    return 1; // sentinel handle
}
int orb_unsubscribe(int) { return 0; }
int orb_copy(const struct orb_metadata *, int, void *) { return -1; }
int orb_check(int, bool *updated) {
    if (updated != nullptr) *updated = false;
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
    const auto *vt = g_stashed_vtable;
    if (vt->open == nullptr || vt->close == nullptr
        || vt->create_publisher == nullptr
        || vt->create_subscriber == nullptr) {
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
    rc = vt->create_publisher(&session, "/unregistered", "T", "H", 0,
                              nullptr, &pubp);
    if (rc != NROS_RMW_RET_TOPIC_NAME_INVALID) {
        std::fprintf(stderr,
                     "create_publisher on unregistered topic returned %d, expected TOPIC_NAME_INVALID\n",
                     rc);
        return 1;
    }

    // Register a synthetic topic and create the publisher.
    static const struct orb_metadata kFakeMeta = {
        /*o_name           =*/ "test_topic",
        /*o_size           =*/ 8,
        /*o_size_no_padding=*/ 8,
        /*o_fields         =*/ "",
    };
    nros_rmw_uorb_clear_registry();
    rc = nros_rmw_uorb_register_topic("/test_topic", "test::Msg", &kFakeMeta);
    if (rc != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "register_topic returned %d\n", rc);
        return 1;
    }

    rc = vt->create_publisher(&session, "/test_topic", "test::Msg", "H", 0,
                              nullptr, &pubp);
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
        std::fprintf(stderr, "expected 1 advertise call, got %d\n",
                     g_orb.advertise_calls);
        return 1;
    }
    if (g_orb.publish_calls != 0) {
        std::fprintf(stderr, "first publish must use advertise, not publish; got %d publish calls\n",
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
        std::fprintf(stderr, "expected 1 publish call, got %d\n",
                     g_orb.publish_calls);
        return 1;
    }
    if (g_orb.last_payload[0] != 0xff) {
        std::fprintf(stderr, "publish payload mismatch\n");
        return 1;
    }

    // Short payload must reject with BUFFER_TOO_SMALL.
    rc = vt->publish_raw(&pubp, payload, sizeof(payload) - 1);
    if (rc != NROS_RMW_RET_BUFFER_TOO_SMALL) {
        std::fprintf(stderr,
                     "short publish returned %d, expected BUFFER_TOO_SMALL\n", rc);
        return 1;
    }

    // destroy_publisher must unadvertise.
    vt->destroy_publisher(&pubp);
    if (g_orb.unadvertise_calls != 1) {
        std::fprintf(stderr, "expected 1 unadvertise call, got %d\n",
                     g_orb.unadvertise_calls);
        return 1;
    }
    if (pubp.backend_data != nullptr) {
        std::fprintf(stderr, "destroy_publisher did not clear backend_data\n");
        return 1;
    }

    // -- Subscriber + service slots still UNSUPPORTED --
    nros_rmw_subscriber_t subp{};
    rc = vt->create_subscriber(&session, "/test_topic", "test::Msg", "H", 0,
                               nullptr, &subp);
    if (rc != NROS_RMW_RET_UNSUPPORTED) {
        std::fprintf(stderr, "create_subscriber returned %d, expected UNSUPPORTED\n", rc);
        return 1;
    }

    vt->close(&session);

    // Null-arg rejection on open.
    rc = vt->open(nullptr, 0, 0, nullptr, nullptr);
    if (rc != NROS_RMW_RET_INVALID_ARGUMENT) {
        std::fprintf(stderr, "open(null out) returned %d, expected INVALID_ARGUMENT\n", rc);
        return 1;
    }

    std::printf("[OK] nros_rmw_uorb K.4.0–K.4.3 (publisher) passes\n");
    return 0;
}
