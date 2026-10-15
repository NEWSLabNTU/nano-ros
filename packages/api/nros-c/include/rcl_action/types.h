/**
 * @file types.h
 * @brief `<rcl_action/types.h>` at upstream's include path (phase-492 W2, RFC-0089).
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

#ifndef NROS_UPSTREAM_PATH_RCL_ACTION_TYPES_H
#define NROS_UPSTREAM_PATH_RCL_ACTION_TYPES_H

#include "rclc/rclc.h"
#include "rclc/detail/rcl_compat.h"

#endif /* NROS_UPSTREAM_PATH_RCL_ACTION_TYPES_H */
