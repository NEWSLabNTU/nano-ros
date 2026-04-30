/**
 * @file service.h
 * @ingroup grp_service
 * @brief Service server API.
 *
 * Create service servers with nros_service_init(), take incoming
 * requests with nros_service_take_request(), and send responses with
 * nros_service_send_response().  For executor-driven dispatch, register
 * a `nros_service_callback_t` at init time.
 */

#ifndef NROS_SERVICE_H
#define NROS_SERVICE_H

/* Type and function definitions live in <nros/nros_generated.h>.
 * This per-module header is kept as a thin shim so existing code that
 * does `#include <nros/service.h>` continues to compile. */
#include "nros/types.h"

#endif /* NROS_SERVICE_H */
