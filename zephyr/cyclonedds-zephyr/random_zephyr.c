/*
 * Copyright (c) 2026, NEWSLab NTU.
 * SPDX-License-Identifier: EPL-2.0 OR BSD-3-Clause
 *
 * Zephyr replacement for
 *   third-party/dds/cyclonedds/src/ddsrt/src/random/posix/random.c
 *
 * The POSIX TU opens /dev/urandom to seed Cyclone's PRNG. Zephyr has no
 * /dev/urandom device, so the original TU is removed from the build (see
 * `list(REMOVE_ITEM)` in zephyr/CMakeLists.txt) and this file ships the
 * same `ddsrt_prng_makeseed` symbol backed by Zephyr's entropy / RNG
 * subsystem via `sys_rand_get()`.
 */

#include <stddef.h>
#include <zephyr/kernel.h>
#include <zephyr/random/random.h>

#include "dds/ddsrt/random.h"

bool ddsrt_prng_makeseed (struct ddsrt_prng_seed *seed)
{
    /* sys_rand_get fills the buffer from Zephyr's configured entropy
     * source (CONFIG_ENTROPY_GENERATOR if a hardware RNG exists, else
     * the timer-based PRNG fallback under CONFIG_TEST_RANDOM_GENERATOR
     * etc.). No error path on the Zephyr side — always returns success.
     */
    sys_rand_get(seed->key, sizeof(seed->key));
    return true;
}
