/**
 * @file error_handling.h
 * @brief `<rcl/error_handling.h>` at upstream's include path (phase-492 W2, RFC-0089).
 *
 * The C API carries rcl's and rclc's names, so a ported file's includes resolve
 * here too. Every upstream-path header brings in the whole nano-ros C API
 * (`<rclc/rclc.h>`) plus the rcl spellings of the return codes
 * (`<rclc/detail/rcl_compat.h>`): the declarations live in the nano-ros headers, and
 * one umbrella is what keeps two spellings of one declaration from existing.
 *
 * Deliberately NOT upstream's include guard. `<rclc/detail/rcl_compat.h>` refuses to
 * compile beside the real `<rcl/types.h>` by testing that guard, and one image
 * never mixes the two.
 */

#ifndef NROS_UPSTREAM_PATH_RCL_ERROR_HANDLING_H
#define NROS_UPSTREAM_PATH_RCL_ERROR_HANDLING_H

#include "rclc/rclc.h"
#include "rclc/detail/rcl_compat.h"

/*
 * phase-515 C5 — rcl's error-state macros over `rcl_set_error_state`. The state
 * itself (`rcl_get_error_string`, `rcl_get_error_state`, `rcl_error_is_set`,
 * `rcl_reset_error`) is declared in the nano-ros headers this one includes;
 * its envelope -- ONE process-wide record, and 256-byte strings -- is stated
 * there. `RCL_SET_ERROR_MSG_WITH_FORMAT_STRING` is not provided: it needs a
 * formatter on every target, and a caller formats into its own buffer instead.
 */
#define RCL_SET_ERROR_MSG(msg)                                                                     \
    do {                                                                                           \
        rcl_set_error_state((msg), __FILE__, __LINE__);                                            \
    } while (0)

#define RCL_CHECK_FOR_NULL_WITH_MSG(value, msg, error_statement)                                   \
    do {                                                                                           \
        if (NULL == (value)) {                                                                     \
            RCL_SET_ERROR_MSG(msg);                                                                \
            error_statement;                                                                       \
        }                                                                                          \
    } while (0)

#define RCL_CHECK_ARGUMENT_FOR_NULL(argument, error_return_type)                                   \
    RCL_CHECK_FOR_NULL_WITH_MSG(argument, #argument " argument is null", return error_return_type)

#endif /* NROS_UPSTREAM_PATH_RCL_ERROR_HANDLING_H */
