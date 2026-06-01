/*
 * Copyright(c) 2026 ZettaScale Technology and others
 *
 * SPDX-License-Identifier: EPL-2.0 OR BSD-3-Clause
 */
#ifndef DDSRT_SYNC_THREADX_H
#define DDSRT_SYNC_THREADX_H

#include <tx_api.h>
#include <stdint.h>

#include "dds/ddsrt/atomics.h"

#if defined (__cplusplus)
extern "C" {
#endif

typedef struct {
  TX_MUTEX mutex;
} ddsrt_mutex_t;

typedef struct {
  TX_SEMAPHORE sem;
  TX_MUTEX lock;
  uint32_t waiters;
} ddsrt_cond_t;

typedef struct {
  TX_MUTEX mutex;
} ddsrt_rwlock_t;

typedef ddsrt_atomic_uint32_t ddsrt_once_t;
#define DDSRT_ONCE_INIT { .v = (1u << 0) }

#if defined (__cplusplus)
}
#endif

#endif /* DDSRT_SYNC_THREADX_H */
