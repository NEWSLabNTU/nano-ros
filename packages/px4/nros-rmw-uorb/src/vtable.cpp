// uORB RMW backend — vtable assembly + register entry point.
//
// Phase 115.K.4.0 (scaffold): every slot points at the matching
// stub in session.cpp / publisher.cpp / subscriber.cpp /
// service.cpp. Stubs return NROS_RMW_RET_UNSUPPORTED so the runtime
// sees a wired-but-inert backend until K.4.1 (session lifecycle),
// K.4.2 (pub/sub data plane), and K.4.3 (type-hash correlation) land.

#include "nros_rmw_uorb.h"

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"

#include "internal.hpp"

namespace {

using namespace nros_rmw_uorb;

// Designated initializers (C++20) — robust to the vtable growing: every slot
// uORB doesn't implement is value-initialized to NULL, which the runtime treats
// as "unsupported / use the fallback." This is the only correct shape now that
// the table carries optional tail slots (Phase 130 non-blocking client, Phase
// 124 zero-copy/borrow/sequence, Phase 231 in-place) that uORB never fills.
// Positional initialization through `call_raw`, in `nros_rmw_vtable_t` field
// order with NO gaps — every slot AFTER `call_raw` (Phase 130 non-blocking
// client, Phase 108 events, Phase 110 deadline, Phase 124 zero-copy/borrow/
// sequence/streamed, Phase 124.F ping, Phase 231 in-place) is left to C++
// aggregate value-initialization (NULL), which the runtime treats as
// "unsupported." Designated initializers would be cleaner but need C++20; this
// crate is C++14 (CMAKE_CXX_STANDARD 14), so keep the gap-free positional form —
// the previous list skipped `send_request_raw`/`try_recv_reply_raw`, which
// shifted every later slot and broke the build.
//
// The trailing-NULL `-Wmissing-field-initializers` is the intended shape here.
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
const nros_rmw_vtable_t kVtable = {
    /* ---- Session lifecycle ---- */
    /*open*/ session_open,
    /*close*/ session_close,
    /*drive_io*/ session_drive_io,
    /* ---- Publisher ---- */
    /*create_publisher*/ publisher_create,
    /*destroy_publisher*/ publisher_destroy,
    /*publish_raw*/ publisher_publish_raw,
    /* ---- Subscriber ---- */
    /*create_subscriber*/ subscriber_create,
    /*destroy_subscriber*/ subscriber_destroy,
    /*try_recv_raw*/ subscriber_try_recv_raw,
    /*has_data*/ subscriber_has_data,
    /* ---- Service Server (uORB: UNSUPPORTED stubs) ---- */
    /*create_service_server*/ service_server_create,
    /*destroy_service_server*/ service_server_destroy,
    /*try_recv_request*/ service_try_recv_request,
    /*has_request*/ service_has_request,
    /*send_reply*/ service_send_reply,
    /* ---- Service Client (uORB: UNSUPPORTED stubs) ---- */
    /*create_service_client*/ service_client_create,
    /*destroy_service_client*/ service_client_destroy,
    /*call_raw*/ service_call_raw,
    // Everything after this point stays NULL (see header comment).
};
#pragma GCC diagnostic pop

} // namespace

extern "C" nros_rmw_ret_t nros_rmw_uorb_register(void) {
    return nros_rmw_cffi_register(&kVtable);
}
