/**
 * @file client.h
 * @ingroup grp_service
 * @brief Service client API.
 *
 * Create service clients with nros_client_init() and call services
 * with nros_client_call() (blocking).
 *
 * The non-blocking paths take the reply: nros_client_take_response()
 * after nros_client_send_request_async(), and
 * nros_client_take_response_raw() on an L1 polling client created with
 * nros_client_init_polling().
 *
 * All four are BYTES. For a typed reply — a deserialised response struct
 * rather than a CDR buffer — the generated per-service header emits
 * `<Srv>_client_send_request()`, `<Srv>_client_take_response()`,
 * `<Srv>_client_call()` and the callback trio
 * `<Srv>_client_handler_t` / `<Srv>_client_handler_init()` /
 * `<Srv>_client_set_response_callback()` (phase-417 W5.e). They are
 * `static inline` forwarders onto the entry points named above — the same
 * transport, the same timeout, one CDR implementation — so nothing here is
 * superseded, and the raw path stays the one to reach for when the payload
 * is not a generated type.
 *
 * The typed send/take helpers take a CALLER-SUPPLIED scratch buffer. That is
 * deliberate: the service pack emits no `_MAX_SERIALIZED_SIZE` constants yet
 * (the message pack does, issue 0896), so a `static inline` with a hidden
 * fixed-size array would reintroduce the silent cliff that work removed. An
 * explicit buffer has no cliff, and no allocator on the delivery path.
 */

#ifndef NROS_CLIENT_H
#define NROS_CLIENT_H

/* Type and function definitions live in <nros/nros_generated.h>.
 * This per-module header is kept as a thin shim so existing code that
 * does `#include <nros/client.h>` continues to compile. */
#include "nros/types.h"
/* phase-417 W5.d — `nros_qos_services_best_effort()`, the profile rclc's
 * service AND client `_best_effort` presets share. One definition, in the
 * service header both halves of the request/response pair already belong to
 * (`@ingroup grp_service` above). */
#include "nros/service.h"

/**
 * @brief rclc's best-effort preset constructor for a service client.
 *
 * phase-417 W5.d; the client-side sibling of rclc_service_init_best_effort(),
 * sharing its profile through nros_qos_services_best_effort() so the two
 * cannot drift — rclc's `client.c` and `service.c` build the same value the
 * same way, and one place to read it here is what keeps that true.
 *
 * `static inline`: no symbol, no writable data.
 *
 * @param[out] client       Zero-initialised client to fill in.
 * @param[in]  node         An initialised node.
 * @param[in]  type_info    Generated service type descriptor.
 * @param[in]  service_name Service name, null-terminated.
 * @return Whatever nros_client_init_with_qos() returns.
 */
static inline nros_ret_t rclc_client_init_best_effort(struct nros_client_t* client,
                                                      const struct nros_node_t* node,
                                                      const struct nros_service_type_t* type_info,
                                                      const char* service_name) {
    struct nros_qos_t qos = nros_qos_services_best_effort();
    return nros_client_init_with_qos(client, node, type_info, service_name, &qos);
}

#endif /* NROS_CLIENT_H */
