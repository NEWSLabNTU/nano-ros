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
#include <stddef.h>
#include <stdint.h>

/** The backend name this registers under. Pass it as the `rmw` selector to
 *  `rclc_support_init_rmw`, and as `$NROS_RMW` if the environment might name
 *  another backend. */
#define NROS_STUB_RMW_NAME "stub"

/** Register the stub under `NROS_STUB_RMW_NAME`. Idempotent (the registry
 *  overwrites a same-name slot). Returns the `rmw_ret_t` the registry gave. */
int32_t nros_stub_rmw_register(void);

/** Issue 1405 — register the SAME stub under another name. A node whose
 *  options name it gets a SECOND session (the executor opens one per distinct
 *  backend), which is how a probe gets two sessions without a second backend. */
int32_t nros_stub_rmw_register_as(const char* name);

/** Issue 1405 — sessions this stub has opened since process start. Session N
 *  (0-based, in open order) is the one `nros_stub_rmw_last_entity_session`
 *  reports as N. */
uint32_t nros_stub_rmw_sessions_opened(void);

/** Issue 1405 — which session (0-based, in open order) the LAST `create_*`
 *  slot was handed, or -1 if none has been. Recorded before the slot refuses,
 *  like the entity name. This is the only place a probe can see the session an
 *  entity REGISTERED on, as opposed to the one its node was bound to. */
int32_t nros_stub_rmw_last_entity_session(void);

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
 *  It answers BOTH halves of 1385: false after `rclc_executor_init` means the
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

/** Issue 1609 — make every `create_*` slot SUCCEED (and count the entity
 *  live) instead of refusing. Off by default; the refusal stays the contract
 *  for every probe that does not ask. */
void nros_stub_rmw_set_accept_entities(bool accept);

/** Entities created in accept mode and not yet destroyed. An entity the
 *  runtime released has left this count, which is how a probe sees it leave
 *  the graph. */
int32_t nros_stub_rmw_live_entities(void);

/** phase-492 W8 — the bytes of the last publish the stub accepted (accept mode
 *  only; outside it every publish is refused), and their length in `*len`. */
const uint8_t* nros_stub_rmw_last_payload(size_t* len);

/** phase-492 W8 — publishes the stub has accepted since process start. */
uint32_t nros_stub_rmw_publish_count(void);

/** Issue 1996 — stage ONE client reply: the next `take_response` on any client
 *  yields these bytes (answering the last request sent), then none until the
 *  next stage. `false` if it does not fit the stub's 256-byte slot. */
bool nros_stub_rmw_stage_reply(const uint8_t* data, size_t len);

/** Issue 1996 — client requests the stub has accepted since process start. */
uint32_t nros_stub_rmw_requests_sent(void);

/** Issue 1940 — the entity kinds the per-entity new-data slots are stored by. */
#define NROS_STUB_ENTITY_SUBSCRIPTION 0
#define NROS_STUB_ENTITY_SERVICE 1
#define NROS_STUB_ENTITY_CLIENT 2

/** Issue 1940 — `false` makes the three `*_set_on_new_*_callback` slots answer
 *  `NROS_RMW_RET_UNSUPPORTED`, as a backend that cannot name the entity does. */
void nros_stub_rmw_set_entity_wake_supported(bool supported);

/** Issue 1940 — call the callback stored for @p kind (an `NROS_STUB_ENTITY_*`),
 *  as a real backend's arrival path would. Returns `false` when none is stored. */
bool nros_stub_rmw_invoke_entity_wake(int kind);

/** Issue 1993 — status events. The stub fills `publisher_event_init` and
 *  `subscription_event_init` and keeps ONE registration per event kind (an
 *  `rmw_event_type_t` value, 0..8) — a probe creates one publisher and one
 *  subscription — and fires it on demand, as a backend's event path would.
 *  A registration outlives the entity's destroy on purpose: that is the late
 *  fire the runtime's generation check must turn into a no-op.
 *
 *  `false` makes `event_type_is_supported` deny every kind, the answer of a
 *  backend that carries no status events (XRCE-DDS, uORB). */
void nros_stub_rmw_set_status_events_supported(bool supported);

/** Issue 1993 — the `deadline_ms` the last registration of @p kind carried,
 *  or -1 if that kind was never registered. */
int64_t nros_stub_rmw_status_event_deadline_ms(int kind);

/** Issue 1993 — fire the registration held for a count-shaped @p kind
 *  (requested deadline missed, message lost, liveliness lost, offered
 *  deadline missed). Returns `false` (and calls nothing) when none is held. */
bool nros_stub_rmw_fire_count_event(int kind, uint32_t total_count, uint32_t total_count_change);

/** Issue 1993 — fire the `LIVELINESS_CHANGED` registration. Returns `false`
 *  when none is held. */
bool nros_stub_rmw_fire_liveliness_changed(uint16_t alive_count, uint16_t not_alive_count,
                                           int16_t alive_count_change,
                                           int16_t not_alive_count_change);

#endif /* NROS_TESTS_STUB_RMW_BACKEND_H */
