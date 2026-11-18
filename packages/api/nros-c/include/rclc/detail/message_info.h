/**
 * @file message_info.h
 * @ingroup grp_subscription
 * @brief `rmw_message_info_t` — the metadata of one received message.
 *
 * Upstream's `rmw_message_info_t` (`<rmw/types.h>`), field for field: when
 * the publisher sent the message and when this subscription received it,
 * both sequence numbers, and the publisher's GID. A message-info subscription
 * callback (`rclc_subscription_message_info_callback_t`) receives one per
 * message.
 *
 * The RMW ABI (`<nros/rmw_entity.h>`) declares the SAME structs under the
 * same guards, so a translation unit that reaches both sees a single
 * definition.
 *
 * A field the backend does not report reads zero (a sequence number too);
 * `publisher_gid.implementation_identifier` is NULL. A backend that reports
 * no metadata at all gets the callback a NULL `rmw_message_info_t *`.
 *
 * Copyright 2026 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_RCLC_MESSAGE_INFO_H
#define NROS_RCLC_MESSAGE_INFO_H

#include <stdbool.h>
#include <stdint.h>

#ifndef NROS_RMW_TIME_POINT_VALUE_T_DEFINED
#define NROS_RMW_TIME_POINT_VALUE_T_DEFINED
/** Nanoseconds since a clock's epoch — upstream's `rmw_time_point_value_t`. */
typedef int64_t rmw_time_point_value_t;
#endif

/* The same value `<nros/rmw_entity.h>` defines; an identical
 * redefinition is legal C, so neither header guards it. */
#define RMW_GID_STORAGE_SIZE 24u

#ifndef NROS_RMW_GID_T_DEFINED
#define NROS_RMW_GID_T_DEFINED
/** Global identifier of a publisher — upstream `rmw_gid_t`. */
typedef struct rmw_gid_t {
    /** Which backend produced this gid. NULL when not reported. */
    const char* implementation_identifier;
    /** The identifier bytes, zero-padded to the full width. All zero = not
     *  reported. */
    uint8_t data[RMW_GID_STORAGE_SIZE];
} rmw_gid_t;
#endif

#ifndef NROS_RMW_MESSAGE_INFO_T_DEFINED
#define NROS_RMW_MESSAGE_INFO_T_DEFINED
/** Metadata of one received message — upstream `rmw_message_info_t`. */
typedef struct rmw_message_info_t {
    /** Publisher's clock at publication, ns. 0 = not reported. */
    rmw_time_point_value_t source_timestamp;
    /** Subscriber's clock at reception, ns. 0 = not reported. */
    rmw_time_point_value_t received_timestamp;
    /** The publisher's sequence number for this message. 0 = not reported. */
    uint64_t publication_sequence_number;
    /** This subscription's reception count. 0 = not reported. */
    uint64_t reception_sequence_number;
    /** Which publisher sent it. All-zero `data` = not reported. */
    rmw_gid_t publisher_gid;
    /** True when the message never left the image. */
    bool from_intra_process;
} rmw_message_info_t;
#endif

#endif /* NROS_RCLC_MESSAGE_INFO_H */
