/**
 * zenoh_generic_platform.h - Generic Platform Header for zenoh-pico
 *
 * This header is included by zenoh-pico when ZENOH_GENERIC is defined.
 * It dispatches to the appropriate platform type definitions based on
 * secondary defines (ZENOH_THREADX, etc.).
 */

#ifndef ZENOH_GENERIC_PLATFORM_H
#define ZENOH_GENERIC_PLATFORM_H

#if defined(ZENOH_THREADX)
#include "zenoh_threadx_platform.h"
#else
#include "zenoh_bare_metal_platform.h"
#endif

#endif /* ZENOH_GENERIC_PLATFORM_H */
