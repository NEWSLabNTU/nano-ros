/* phase-492 W8 — MUST NOT COMPILE (a `cxx-syntax-verdict` row; the consuming
 * test is `tests/c_rosidl_refusals.rs`).
 *
 * `ROSIDL_GET_MSG_TYPE_SUPPORT` is adopted and yields nano-ros's descriptor; the
 * STRUCT `rosidl_message_type_support_t` is not declared (RFC-0089 2026-10-12),
 * because its members are a dispatcher the descriptor does not have. A ported
 * line that names the struct must fail on the struct's name. */
#include <std_msgs/msg/string.h>

const void* refused(void);
const void* refused(void) {
    const rosidl_message_type_support_t* ts = ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String);
    return ts;
}
