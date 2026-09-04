/* RFC-0088 D5 / phase-421 W6 — the NEGATIVE half of the C format check.
 *
 * EXPECTED TO FAIL TO COMPILE. `check-c` runs it as an expected-failure, the
 * same shape as `param_deprecation_probe.c`: a clean compile here means the
 * per-message `_Static_assert` codegen emits has stopped asserting anything, so
 * a C entry could publish a message in an encoding the linked backend does not
 * speak and find out on the wire.
 *
 * The mismatch is spelled the way a non-CDR codegen pack would spell it — the
 * message's own `NROS_MSG_FORMAT_ID_<type>` naming a different format from the
 * image's. Today every in-tree pack emits CDR, so there is no generated header
 * that disagrees; hardcoding the discriminant here is what makes the refusal
 * testable before a second pack exists (RFC-0088 W5 ships one).
 *
 * Copyright 2026 nros contributors
 * Licensed under Apache-2.0
 */

#include <nros/nros.h>

/* Belt and braces: if the image itself were ever uORB, this probe would be
   asserting a TRUE condition and would compile — reporting a broken check that
   is in fact fine. Fail loudly instead of quietly inverting. */
#if NROS_SERIALIZATION_FORMAT_ID != NROS_SERIALIZATION_FORMAT_ID_CDR
#error "this probe assumes a CDR image; pick a discriminant the image does NOT speak"
#endif

/* A uORB-encoded message (RFC-0011: the PX4 struct verbatim, no CDR anywhere)
   in an image whose backend speaks CDR. */
#define NROS_MSG_FORMAT_ID_px4_msgs_msg_vehicle_status NROS_SERIALIZATION_FORMAT_ID_UORB

/* THIS is the line that must not compile. */
NROS_ASSERT_MESSAGE_FORMAT(NROS_MSG_FORMAT_ID_px4_msgs_msg_vehicle_status,
                           "px4_msgs/VehicleStatus");

/* ...and this is the C entry that would have published it. Kept so the probe
   describes a real mistake rather than a bare assertion in a vacuum. */
nros_ret_t nros_probe_publish_vehicle_status(struct nros_publisher_t* publisher,
                                             const uint8_t* payload, size_t payload_len);

nros_ret_t nros_probe_publish_vehicle_status(struct nros_publisher_t* publisher,
                                             const uint8_t* payload, size_t payload_len) {
    return nros_publish_raw(publisher, payload, payload_len);
}
