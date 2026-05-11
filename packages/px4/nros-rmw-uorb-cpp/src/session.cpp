// Session lifecycle — uORB has no global session object (in-process,
// shared broker). The session struct just carries the node-name /
// namespace for diagnostics; entity-create paths key off `backend_data`
// being non-null.
//
// Phase 115.K.4.0 (scaffold): all entries return UNSUPPORTED so the
// runtime treats the backend as wired-but-inert. K.4.1 will replace
// open/close with allocating a small UorbSessionState (node_name +
// namespace heapless::String-equivalent) and stash the pointer in
// `out->backend_data`. drive_io stays a no-op (uORB push-based via
// orb_register_callback signals).

#include "internal.hpp"

#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"

namespace nros_rmw_uorb {

nros_rmw_ret_t session_open(const char * /*locator*/, uint8_t /*mode*/,
                            uint32_t /*domain_id*/, const char * /*node_name*/,
                            nros_rmw_session_t * /*out*/) {
    return NROS_RMW_RET_UNSUPPORTED;
}

nros_rmw_ret_t session_close(nros_rmw_session_t * /*session*/) {
    return NROS_RMW_RET_UNSUPPORTED;
}

nros_rmw_ret_t session_drive_io(nros_rmw_session_t * /*session*/,
                                int32_t /*timeout_ms*/) {
    // K.4.1 target: return NROS_RMW_RET_OK unconditionally. uORB is
    // push-based — orb_register_callback fires from the broker
    // thread and signals the per-subscriber ringbuffer waker; the
    // runtime's drive_io has no work to do for this backend.
    return NROS_RMW_RET_UNSUPPORTED;
}

} // namespace nros_rmw_uorb
