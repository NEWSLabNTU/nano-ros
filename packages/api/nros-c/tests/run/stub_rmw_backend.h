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

/** Issue 2074 — register the stub under @p name WITHOUT its `take_with_info`
 *  slot, so a sample carries no metadata channel at all (the XRCE / uORB
 *  case). Lets a probe tell "nothing reported" apart from "zeros reported".
 *  Issue 2081 — `take_request_with_info` is left out too, and (issue 2136)
 *  `take_response_with_info`. */
int32_t nros_stub_rmw_register_without_message_info(const char* name);

/** Issue 1405 — sessions this stub has opened since process start. Session N
 *  (0-based, in open order) is the one `nros_stub_rmw_last_entity_session`
 *  reports as N. */
uint32_t nros_stub_rmw_sessions_opened(void);

/** Issue 1405 — which session (0-based, in open order) the LAST `create_*`
 *  slot was handed, or -1 if none has been. Recorded before the slot refuses,
 *  like the entity name. This is the only place a probe can see the session an
 *  entity REGISTERED on, as opposed to the one its node was bound to. */
int32_t nros_stub_rmw_last_entity_session(void);

/** Issue 2123 — how many properties session N (0-based, in open order) was
 *  opened with: 0 for none, -1 when no such session has opened or it carried
 *  more rows than the stub records (4). */
int32_t nros_stub_rmw_session_property_count(uint32_t session);

/** Issue 2123 — row @p i of the properties session N was opened with, as the
 *  stub's own COPIES (the runtime lends the rows for the open only). False when
 *  out of range. */
bool nros_stub_rmw_session_property(uint32_t session, size_t i, const char** key,
                                    const char** value);

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

/** phase-515 P4 — the type name the last publisher / subscription was created
 *  with ("" if none), and through `type_hash` its hash. */
const char* nros_stub_rmw_last_entity_type(const char** type_hash);

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

/** phase-515 C8 — what `service_server_is_available` answers: `1` a server is
 *  visible, `0` none is, `-1` (the default) the backend cannot tell. */
void nros_stub_rmw_set_server_available(int available);

/** Issue 1996 — client requests the stub has accepted since process start. */
uint32_t nros_stub_rmw_requests_sent(void);

/** Issue 2009 — stage a reply for the request sent as @p seq. Replies are taken
 *  oldest-staged first, whatever their ids, so staging B's before A's delivers
 *  them out of order. `false` when the stub's 8 slots are full or the reply
 *  does not fit 256 bytes.
 *
 *  Issue 2011 — a reply is taken only by the CLIENT that sent @p seq (each
 *  accept-mode client has its own identity), so an action client's three
 *  sub-clients, or two clients, never take each other's replies. A @p seq the
 *  stub has no sender for (never sent, or older than its last 64 requests)
 *  goes to whichever client asks first. */
bool nros_stub_rmw_stage_reply_for(int64_t seq, const uint8_t* data, size_t len);

/** Issue 2136 — the request-writer GUID `take_response_with_info` reports
 *  beside each reply's id (NULL or all zero: not reported, the default).
 *  Cleared by `nros_stub_rmw_reset_client`. */
void nros_stub_rmw_set_reply_writer_guid(const int8_t guid[16]);

/** Issue 2009 — the id the stub assigned to the most recent request. */
int64_t nros_stub_rmw_last_request_seq(void);

/** Issue 2009 — refuse a send (`NROS_RMW_RET_WOULD_BLOCK`, nothing sent) once
 *  @p max requests are outstanding (sent and not yet answered by a taken
 *  reply), as a real backend refuses past its own limit. 0 = no cap. */
void nros_stub_rmw_set_max_outstanding(uint32_t max);

/** Issue 2009 — `false`: neither `send_request` nor `take_response` writes an
 *  id, so the runtime sees `NROS_RMW_SEQUENCE_ID_UNKNOWN` — a backend that
 *  cannot assign ids. */
void nros_stub_rmw_set_ids_supported(bool supported);

/** Issue 2009 — replies staged and not yet taken. */
size_t nros_stub_rmw_staged_replies(void);

/** Issue 2009 — drop staged replies and restore the defaults (no cap, ids
 *  supported), so one probe's leftovers cannot answer the next. */
void nros_stub_rmw_reset_client(void);

/** Issue 2015 — stage a REQUEST for the service server whose name ends with
 *  @p service_suffix (e.g. `"/_action/send_goal"`), taken with id @p seq.
 *  Every accept-mode service server and subscription remembers its name, so a
 *  staged request (or sample) reaches only the entity it names. `false` when
 *  the stub's 16 slots are full or the bytes do not fit 512. Cleared by
 *  `nros_stub_rmw_reset_client`. */
bool nros_stub_rmw_stage_request(const char* service_suffix, int64_t seq, const uint8_t* data,
                                 size_t len);

/** Issue 2015 — stage a SAMPLE for the subscription whose topic ends with
 *  @p topic_suffix (e.g. `"/_action/feedback"`). Same slots as requests. */
bool nros_stub_rmw_stage_sample(const char* topic_suffix, const uint8_t* data, size_t len);

/** Issue 2036 — the metadata a staged sample carries (`rmw_message_info_t`'s
 *  fields, without the RMW headers). */
typedef struct nros_stub_message_info_t {
    int64_t source_timestamp;
    int64_t received_timestamp;
    uint64_t publication_sequence_number;
    uint64_t reception_sequence_number;
    uint8_t publisher_gid[24];
} nros_stub_message_info_t;

/** Issue 2036 — `nros_stub_rmw_stage_sample` plus the metadata the backend's
 *  `take_with_info` slot reports with it. */
bool nros_stub_rmw_stage_sample_info(const char* topic_suffix, const uint8_t* data, size_t len,
                                     const nros_stub_message_info_t* info);

/** Issue 2081 — the metadata a staged request carries (`rmw_service_info_t`'s
 *  fields, without the RMW headers). Zero means "not reported", as there. */
typedef struct nros_stub_service_info_t {
    int64_t source_timestamp;
    int64_t received_timestamp;
    int8_t writer_guid[16];
    int64_t sequence_number;
} nros_stub_service_info_t;

/** Issue 2081 — `nros_stub_rmw_stage_request` plus the metadata the backend's
 *  `take_request_with_info` slot reports with it. */
bool nros_stub_rmw_stage_request_info(const char* service_suffix, int64_t seq, const uint8_t* data,
                                      size_t len, const nros_stub_service_info_t* info);

/** Issue 2015 — the newest response a service server whose name ends with
 *  @p service_suffix sent: its bytes (NULL if none), id and length. */
const uint8_t* nros_stub_rmw_last_response(const char* service_suffix, int64_t* seq, size_t* len);

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

/** Issue 2060 — subscription `has_data` calls since process start. The
 *  executor's readiness scan is the only caller in a probe, so this is how a
 *  probe sees whether a pass ASKED a subscription or read its pushed bit. */
uint32_t nros_stub_rmw_subscription_has_data_calls(void);

/** Issue 2060 — fire the count-shaped @p kind registration from inside the
 *  NEXT subscription `has_data` call, once — where zenoh and Cyclone poll the
 *  events about nothing arriving (a deadline passed, the last publisher left).
 *  Nothing fires until a `has_data` actually runs. */
void nros_stub_rmw_stage_polled_count_event(int kind, uint32_t total_count,
                                            uint32_t total_count_change);

#endif /* NROS_TESTS_STUB_RMW_BACKEND_H */
