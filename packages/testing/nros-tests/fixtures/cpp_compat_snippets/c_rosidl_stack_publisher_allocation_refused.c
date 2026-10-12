/* phase-492 W8 — MUST NOT COMPILE (`tests/c_rosidl_refusals.rs`).
 *
 * `rmw_publisher_allocation_t` is declared INCOMPLETE: `rcl_publish(&pub, &msg,
 * NULL)` compiles, a pre-allocation object does not, because nano-ros has none
 * to hand out. */
#include <rcl/publisher.h>
#include <std_msgs/msg/string.h>

rcl_ret_t refused(const rcl_publisher_t* publisher, const std_msgs__msg__String* msg);
rcl_ret_t refused(const rcl_publisher_t* publisher, const std_msgs__msg__String* msg) {
    rmw_publisher_allocation_t allocation;
    return rcl_publish(publisher, msg, &allocation);
}
