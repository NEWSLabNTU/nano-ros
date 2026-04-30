/**
 * @file types.h
 * @ingroup grp_types
 * @brief Shared types and constants for the nros C API.
 *
 * Type and constant definitions live in `nros_generated.h` (the
 * single source of truth for field layout) and `nros_config_generated.h`
 * (opaque-storage size macros). This file is a thin wrapper that
 * pulls in both.
 *
 * Keeping `types.h` preserves backward compatibility for downstream
 * code that already includes it. New code can include
 * `<nros/nros_generated.h>` (or any specific module header) directly.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_TYPES_H
#define NROS_TYPES_H

#include "nros/visibility.h"
#include "nros/nros_config_generated.h"
#include "nros/nros_generated.h"

#endif /* NROS_TYPES_H */
