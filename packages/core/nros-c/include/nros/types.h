/**
 * @file types.h
 * @brief Shared types and constants for the nros C API.
 *
 * Phase 91.C1: this file is now a thin transitional shim. The actual
 * type and constant definitions come from `nros_generated.h`
 * (cbindgen-emitted from Rust `#[repr(C)]` types — the field-exact
 * single source of truth) and `nros_config_generated.h` (build-script-
 * emitted opaque-storage size macros).
 *
 * Keeping `types.h` as a wrapper preserves backward compatibility for
 * downstream code that already includes it. New code can include
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
