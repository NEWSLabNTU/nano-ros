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
/* RFC-0088 D5 — NROS_SERIALIZATION_FORMAT{,_ID} plus the per-message
   compile-time assertion generated message headers emit. Included here so a
   generated header reaches it through its single `<nros/types.h>` include. */
#include "nros/serialization_format.h"
/* issue 1531 — LAST, and deliberately so: this header defines function-like
   macros named after entry points `nros_generated.h` DECLARES, so it has to come
   after that declaration or it would eat it. It is here rather than in
   `<nros/init.h>` because every consumer must get it — two TUs in one image
   disagreeing about whether the baked RMW rung is passed is issue 1530's failure
   with a narrower blast radius. */
#include "nros/baked_rmw.h"

#endif /* NROS_TYPES_H */
