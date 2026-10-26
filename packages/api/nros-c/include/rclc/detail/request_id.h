/**
 * @file request_id.h
 * @ingroup grp_service
 * @brief `rmw_request_id_t` — the identity of one service request (issue 2081).
 *
 * Upstream's `rmw_request_id_t` (`<rmw/types.h>`), field for field: the
 * requesting client's writer GUID and the sequence number it assigned. A
 * request-id service callback (`rclc_service_raw_callback_with_request_id_t`)
 * receives one per request.
 *
 * The RMW ABI (`<nros/rmw_entity.h>`) and rclcpp's `service.hpp` declare the
 * SAME struct under the same guard, so a translation unit that reaches more
 * than one of them sees a single definition.
 *
 * A backend that does not report who sent a request leaves every field zero;
 * an all-zero `writer_guid` means "not reported".
 *
 * Copyright 2026 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_RCLC_REQUEST_ID_H
#define NROS_RCLC_REQUEST_ID_H

#include <stdint.h>

#ifndef NROS_RMW_REQUEST_ID_T_DEFINED
#define NROS_RMW_REQUEST_ID_T_DEFINED
/** Identity of one service request — upstream `rmw_request_id_t`. */
typedef struct rmw_request_id_t {
    /** The requesting client's identity. All zero = not reported. */
    int8_t writer_guid[16];
    /** The client-assigned sequence number of this request. */
    int64_t sequence_number;
} rmw_request_id_t;
#endif

#endif /* NROS_RCLC_REQUEST_ID_H */
