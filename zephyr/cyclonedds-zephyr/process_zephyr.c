/*
 * Copyright (c) 2026, NEWSLab NTU.
 * SPDX-License-Identifier: EPL-2.0 OR BSD-3-Clause
 *
 * Zephyr replacement for Cyclone DDS' POSIX process TU.
 *
 * The POSIX implementation reads /proc/self/cmdline with fopen().
 * Zephyr's libc routes that through fs_open(), which aborts when no
 * filesystem is mounted. Cyclone only needs a stable process id/name for
 * participant metadata and GUID fallback entropy, so avoid filesystem use.
 */

#include <stdint.h>

#include <zephyr/kernel.h>

#include "dds/ddsrt/heap.h"
#include "dds/ddsrt/process.h"
#include "dds/ddsrt/string.h"

ddsrt_pid_t ddsrt_getpid(void)
{
    return (ddsrt_pid_t)(uintptr_t)k_current_get();
}

char *ddsrt_getprocessname(void)
{
    return ddsrt_strdup("zephyr");
}
