/* A minimal RMW backend for C run-tests that need an executor and no wire.
 *
 * An executor needs a session and a session needs a registered backend, so a
 * probe about executor BEHAVIOUR — timers, callback ordering, spin accounting —
 * cannot run without one. The alternative is a real backend, which means a
 * router or an agent and takes the probe out of `just check c`.
 *
 * The vtable answers the seventeen slots `first_missing_vtable_slot` requires
 * (issue 0332 rejects an incomplete vtable at registration). Session lifecycle
 * succeeds; every entity slot answers `NROS_RMW_RET_UNSUPPORTED`, so a test
 * that accidentally starts publishing gets a loud refusal rather than a silent
 * pass.
 *
 * The TU that links this owns `nros_app_register_backends` — there is no weak
 * default on the C path — and should call `nros_stub_rmw_register()` from it.
 */

#ifndef NROS_TESTS_STUB_RMW_BACKEND_H
#define NROS_TESTS_STUB_RMW_BACKEND_H

#include <stdint.h>

/** The backend name this registers under. Pass it as the `rmw` selector to
 *  `nros_support_init_rmw`, and as `$NROS_RMW` if the environment might name
 *  another backend. */
#define NROS_STUB_RMW_NAME "stub"

/** Register the stub under `NROS_STUB_RMW_NAME`. Idempotent (the registry
 *  overwrites a same-name slot). Returns the `rmw_ret_t` the registry gave. */
int32_t nros_stub_rmw_register(void);

/** How many times the executor drove the stub's I/O. A spin that never reached
 *  the backend is a probe that measured nothing. */
uint32_t nros_stub_rmw_drive_io_calls(void);

#endif /* NROS_TESTS_STUB_RMW_BACKEND_H */
