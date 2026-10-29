/**
 * @file primitives_sequence.h
 * @brief rosidl's primitive sequences, in rosidl's shape (phase-492 W7).
 *
 * `rosidl_runtime_c__<prim>__Sequence` is `{data, size, capacity}` over memory
 * the CALLER owns. The names are rosidl's, after its `.msg` -> IDL mapping:
 * `float32[]` is float__Sequence, `float64[]` double__Sequence, `byte[]`
 * octet__Sequence, `bool[]` boolean__Sequence, `char[]` uint8__Sequence;
 * the `.msg` spellings are aliased as rosidl aliases them.
 *
 * Guarded with upstream's include guard: a translation unit that already has
 * rosidl's definitions (identical layout) keeps them, and this is the ONE
 * definition site in nano-ros. The allocating `__Sequence__init` / `__fini`
 * are not declared (see <rosidl_runtime_c/string.h>).
 */

#ifndef ROSIDL_RUNTIME_C__PRIMITIVES_SEQUENCE_H_
#define ROSIDL_RUNTIME_C__PRIMITIVES_SEQUENCE_H_

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/** Upstream's own generator macro, same name and shape. */
#define ROSIDL_RUNTIME_C__PRIMITIVE_SEQUENCE(STRUCT_NAME, TYPE_NAME)                               \
    typedef struct rosidl_runtime_c__##STRUCT_NAME##__Sequence {                                   \
        TYPE_NAME* data;                                                                           \
        size_t size;                                                                               \
        size_t capacity;                                                                           \
    } rosidl_runtime_c__##STRUCT_NAME##__Sequence;

ROSIDL_RUNTIME_C__PRIMITIVE_SEQUENCE(float, float)
ROSIDL_RUNTIME_C__PRIMITIVE_SEQUENCE(double, double)
ROSIDL_RUNTIME_C__PRIMITIVE_SEQUENCE(long_double, long double)
ROSIDL_RUNTIME_C__PRIMITIVE_SEQUENCE(char, signed char)
ROSIDL_RUNTIME_C__PRIMITIVE_SEQUENCE(wchar, uint16_t)
ROSIDL_RUNTIME_C__PRIMITIVE_SEQUENCE(boolean, bool)
ROSIDL_RUNTIME_C__PRIMITIVE_SEQUENCE(octet, uint8_t)
ROSIDL_RUNTIME_C__PRIMITIVE_SEQUENCE(uint8, uint8_t)
ROSIDL_RUNTIME_C__PRIMITIVE_SEQUENCE(int8, int8_t)
ROSIDL_RUNTIME_C__PRIMITIVE_SEQUENCE(uint16, uint16_t)
ROSIDL_RUNTIME_C__PRIMITIVE_SEQUENCE(int16, int16_t)
ROSIDL_RUNTIME_C__PRIMITIVE_SEQUENCE(uint32, uint32_t)
ROSIDL_RUNTIME_C__PRIMITIVE_SEQUENCE(int32, int32_t)
ROSIDL_RUNTIME_C__PRIMITIVE_SEQUENCE(uint64, uint64_t)
ROSIDL_RUNTIME_C__PRIMITIVE_SEQUENCE(int64, int64_t)

/* rosidl's aliases for the `.msg` spellings. */
typedef rosidl_runtime_c__boolean__Sequence rosidl_runtime_c__bool__Sequence;
typedef rosidl_runtime_c__octet__Sequence rosidl_runtime_c__byte__Sequence;
typedef rosidl_runtime_c__float__Sequence rosidl_runtime_c__float32__Sequence;
typedef rosidl_runtime_c__double__Sequence rosidl_runtime_c__float64__Sequence;

#ifdef __cplusplus
}
#endif

#endif /* ROSIDL_RUNTIME_C__PRIMITIVES_SEQUENCE_H_ */
