/**
 * @file data_types.h
 * @brief `<rcl_lifecycle/data_types.h>` at upstream's include path (phase-492 W2, RFC-0089).
 *
 * The C API carries rcl's and rclc's names, so a ported file's includes resolve
 * here too. Every upstream-path header brings in the whole nano-ros C API
 * (`<nros/nros.h>`) plus the rcl spellings of the return codes
 * (`<nros/rcl_compat.h>`): the declarations live in the nano-ros headers, and
 * one umbrella is what keeps two spellings of one declaration from existing.
 *
 * Deliberately NOT upstream's include guard. `<nros/rcl_compat.h>` refuses to
 * compile beside the real `<rcl/types.h>` by testing that guard, and one image
 * never mixes the two.
 */

#ifndef NROS_UPSTREAM_PATH_RCL_LIFECYCLE_DATA_TYPES_H
#define NROS_UPSTREAM_PATH_RCL_LIFECYCLE_DATA_TYPES_H

#include "nros/nros.h"
#include "nros/rcl_compat.h"

#endif /* NROS_UPSTREAM_PATH_RCL_LIFECYCLE_DATA_TYPES_H */
