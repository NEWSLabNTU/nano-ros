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

#include <stdbool.h>
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

/** Issue 1385 — does this backend currently hold a runtime wake callback?
 *
 *  The executor's side of `set_wake_callback` is otherwise unobservable from a
 *  test: `has_async_wake` is private and the only symptom of a missing install
 *  is a spin that waits longer, which is a number and not a fact. This is the
 *  fact — the backend's own slot, read back.
 *
 *  It answers BOTH halves of 1385: false after `nros_executor_init` means the
 *  C path installed nothing, and true after `rclc_executor_fini` means a
 *  backend is holding a callback into storage the fini has zero-filled. */
bool nros_stub_rmw_wake_cb_installed(void);

/** Issue 1385 — invoke the stored wake callback, as a real backend's worker
 *  thread or ISR would on an arrival. Returns false (and calls nothing) when
 *  no callback is installed.
 *
 *  This is the arrival a C executor could not be woken by: it is signalled
 *  from OUTSIDE the `drive_io` the executor is parked in, which is exactly the
 *  path `drive_io(full timeout)` cannot observe. */
bool nros_stub_rmw_invoke_wake(void);

/** The topic / service name the LAST `create_*` slot was called with, or "" if
 *  none has been. The slots still refuse, but they refuse AFTER the runtime has
 *  resolved the name, so this is the only place a C test can read the WIRE name
 *  a node computed rather than the source spelling it passed in (issue 1384). */
const char* nros_stub_rmw_last_entity_name(void);

/** Reset [`nros_stub_rmw_last_entity_name`] to "", so a later read cannot be
 *  satisfied by an earlier call's value. */
void nros_stub_rmw_clear_last_entity_name(void);

#endif /* NROS_TESTS_STUB_RMW_BACKEND_H */
