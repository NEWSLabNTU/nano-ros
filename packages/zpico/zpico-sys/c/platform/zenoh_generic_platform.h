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
#include "threadx/platform.h"
#else
#include "bare-metal/platform.h"
#endif

#endif /* ZENOH_GENERIC_PLATFORM_H */
