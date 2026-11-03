// Phase 115.K.4.2-subscriber-push — the push-wake callback ABI for a build
// WITHOUT the PX4 glue (the host test double): registration is unsupported,
// so every subscription polls. Such a session states no readiness form and
// the runtime refuses it at open (RFC-0113 D5).
//
// Linked ONLY when the glue is not (`NROS_RMW_UORB_NO_GLUE_SOURCES`). These
// were weak stubs beside `px4_callback_glue.cpp`'s strong definitions until
// phase-521 W8 (d), and in a static archive the stub won: the linker pulls
// the first member that resolves the symbol, and nothing else references the
// glue object, so the PX4 module shipped without its push-wake glue. A test
// driver that links the library supplies its own definitions
// (`tests/register_smoke.cpp`); this member is then never pulled.

#include "uorb_abi.hpp"

extern "C" {

int nros_orb_register_callback(const struct orb_metadata * /*meta*/,
                               uint8_t /*instance*/,
                               int /*handle*/,
                               nros_orb_callback_t /*cb*/,
                               void * /*arg*/) {
    return -1;
}

int nros_orb_unregister_callback(int /*handle*/) {
    return -1;
}

} // extern "C"
