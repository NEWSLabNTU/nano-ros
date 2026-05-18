/*
 * Copyright (c) 2026, NEWSLab NTU.
 * SPDX-License-Identifier: EPL-2.0 OR BSD-3-Clause
 *
 * Replacement stubs for the SHM (iceoryx) functions that
 * `ddsi_shm_transport.c` would normally define. We dropped that TU
 * from the Zephyr build under DDS_HAS_SHM=0; downstream Cyclone code
 * still references the public surface via `DDS_EXPORT` headers, so
 * provide no-op implementations to satisfy the linker.
 *
 * Runtime is never expected to enter these — call sites are gated
 * by SHM=0 — but the symbols must exist for the static-link image.
 */

#include "dds/ddsi/ddsi_shm_transport.h"

iox_sub_context_t ** iox_sub_context_ptr(iox_sub_t sub)
{
    (void)sub;
    return NULL;
}

void iox_sub_context_init(iox_sub_context_t * context)
{
    (void)context;
}

void iox_sub_context_fini(iox_sub_context_t * context)
{
    (void)context;
}

void shm_lock_iox_sub(iox_sub_t sub)
{
    (void)sub;
}

void shm_unlock_iox_sub(iox_sub_t sub)
{
    (void)sub;
}
