/**
 * @file string.h
 * @brief rosidl's C string, in rosidl's shape (phase-492 W7, RFC-0089).
 *
 * `rosidl_runtime_c__String` and `rosidl_runtime_c__String__Sequence` are
 * `{data, size, capacity}` over memory the CALLER owns: `data` holds `size`
 * characters plus a terminating NUL, inside `capacity` bytes. The micro-ROS
 * idiom works unchanged:
 *
 *     char buf[64];
 *     msg.data.data = buf;
 *     msg.data.size = 0;
 *     msg.data.capacity = sizeof buf;
 *
 * Guarded with upstream's include guard, so a translation unit that already
 * has rosidl's definition (identical layout) keeps it, and this is the ONE
 * definition site in nano-ros.
 *
 * NOT declared: rosidl's `rosidl_runtime_c__String__init` / `__fini` /
 * `__assign` / `__assignn` and the `__Sequence__init` / `__fini` pair. They
 * allocate in rosidl, and nano-ros never allocates on a message's behalf
 * (RFC-0105 D10). A ported call fails to compile and names the alternative:
 * point `data` at your own buffer, or wire a whole message with the generated
 * `<Msg>__init_in(&msg, &storage)`.
 */

#ifndef ROSIDL_RUNTIME_C__STRING_H_
#define ROSIDL_RUNTIME_C__STRING_H_

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/** A string: `size` characters at `data`, NUL-terminated, in `capacity` bytes. */
typedef struct rosidl_runtime_c__String {
    char* data;
    size_t size;
    size_t capacity;
} rosidl_runtime_c__String;

/** A sequence of strings, each with its own caller-owned bytes. */
typedef struct rosidl_runtime_c__String__Sequence {
    rosidl_runtime_c__String* data;
    size_t size;
    size_t capacity;
} rosidl_runtime_c__String__Sequence;

#define rosidl_runtime_c__String__init(...) rosidl_allocates__point_data_at_your_own_buffer
#define rosidl_runtime_c__String__fini(...) rosidl_allocates__point_data_at_your_own_buffer
#define rosidl_runtime_c__String__assign(...) rosidl_allocates__point_data_at_your_own_buffer
#define rosidl_runtime_c__String__assignn(...) rosidl_allocates__point_data_at_your_own_buffer
#define rosidl_runtime_c__String__Sequence__init(...)                                              \
    rosidl_allocates__point_data_at_your_own_buffer
#define rosidl_runtime_c__String__Sequence__fini(...)                                              \
    rosidl_allocates__point_data_at_your_own_buffer

#ifdef __cplusplus
}
#endif

#endif /* ROSIDL_RUNTIME_C__STRING_H_ */
