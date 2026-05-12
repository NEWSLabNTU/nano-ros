// Phase 115.K.4.2-subscriber-push — weak default impl of the
// push-wake callback ABI.
//
// Two build paths:
//
//   1. Standalone / host smoke test (default):
//      these weak definitions return -1 (unsupported). The test
//      driver overrides them with strong symbols that stash
//      (cb, arg) so the test can fire the callback synthetically
//      (see `tests/register_smoke.cpp`).
//
//   2. PX4 module build (NROS_RMW_UORB_LINK_PX4=ON):
//      `px4_callback_glue.cpp` provides strong definitions that
//      wrap `uORB::SubscriptionCallbackWorkItem`. The weak
//      symbols here are overridden at link time.
//
// Keeping the default in its own TU lets the linker drop these
// weak symbols entirely if a strong override is present, which
// is what we want — the standalone path never wants the
// "unsupported" stub to ship in production binaries.

#include "uorb_abi.hpp"

extern "C" {

#if defined(__GNUC__) || defined(__clang__)
__attribute__((weak))
#endif
int nros_orb_register_callback(const struct orb_metadata * /*meta*/,
                               uint8_t /*instance*/,
                               int /*handle*/,
                               nros_orb_callback_t /*cb*/,
                               void * /*arg*/) {
    return -1;
}

#if defined(__GNUC__) || defined(__clang__)
__attribute__((weak))
#endif
int nros_orb_unregister_callback(int /*handle*/) {
    return -1;
}

} // extern "C"
