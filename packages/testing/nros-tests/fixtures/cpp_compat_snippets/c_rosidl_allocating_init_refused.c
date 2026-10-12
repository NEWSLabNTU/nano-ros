/* phase-492 W7 — MUST NOT COMPILE (`tests/c_rosidl_refusals.rs`).
 *
 * rosidl's `<Msg>__init` allocates, so nano-ros does not declare it: the call
 * fails, and the diagnostic names `std_msgs__msg__String__init_in`, the
 * caller-owned alternative. */
#include <std_msgs/msg/string.h>

void refused(void);
void refused(void) {
    std_msgs__msg__String msg;
    std_msgs__msg__String__init(&msg);
}
