/* phase-492 W8 — MUST NOT COMPILE (`tests/c_rosidl_refusals.rs`).
 *
 * Reading a rosidl typesupport member through the adopted macro: the descriptor
 * has no `typesupport_identifier` (nor `data`, nor `func`), so introspection
 * code written against rosidl fails here by construction rather than reading
 * something else. */
#include <std_msgs/msg/string.h>

const char* refused(void);
const char* refused(void) {
    return ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String)->typesupport_identifier;
}
