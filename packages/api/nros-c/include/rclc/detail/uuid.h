/**
 * @file uuid.h
 * @ingroup grp_action
 * @brief `unique_identifier_msgs__msg__UUID`, the goal id every action
 *        envelope and goal handle carries (phase-515 C4).
 *
 * rosidl's struct, member for member. The action envelopes a generated
 * action header emits (`<Action>_SendGoal_Request`, `_FeedbackMessage`) and
 * rclc's `rclc_action_goal_handle_t` embed it, and they cannot rely on the
 * user's build having generated `unique_identifier_msgs` before them.
 *
 * Defined under `NROS_DEFINED_unique_identifier_msgs__msg__UUID`, the same
 * guard the generated `unique_identifier_msgs` header wraps its typedef in, so
 * whichever is included first defines it and the other is a no-op. Codegen
 * emits that guard around EVERY struct it declares (`NROS_DEFINED_<name>`);
 * this is the one the runtime also needs.
 */

#ifndef NROS_RCLC_DETAIL_UUID_H
#define NROS_RCLC_DETAIL_UUID_H

#include <stdint.h>

#ifndef NROS_DEFINED_unique_identifier_msgs__msg__UUID
#define NROS_DEFINED_unique_identifier_msgs__msg__UUID
/// rosidl's `unique_identifier_msgs/msg/UUID`: 16 bytes.
typedef struct unique_identifier_msgs__msg__UUID {
    uint8_t uuid[16];
} unique_identifier_msgs__msg__UUID;
#endif

#endif /* NROS_RCLC_DETAIL_UUID_H */
