/*
 * Copyright(c) 2026 ZettaScale Technology and others
 *
 * SPDX-License-Identifier: EPL-2.0 OR BSD-3-Clause
 */
#ifndef DDSRT_THREADS_THREADX_H
#define DDSRT_THREADS_THREADX_H

#include <inttypes.h>
#include <stdint.h>
#include <tx_api.h>

#define DDSRT_HAVE_THREAD_SETNAME (0)
#define DDSRT_HAVE_THREAD_LIST (0)

#if defined(__cplusplus)
extern "C" {
#endif

typedef struct {
  TX_THREAD *thread;
  void *context;
} ddsrt_thread_t;

typedef uintptr_t ddsrt_tid_t;
typedef TX_THREAD *ddsrt_thread_list_id_t;
#define PRIdTID PRIuPTR

#if defined(__cplusplus)
}
#endif

#endif /* DDSRT_THREADS_THREADX_H */
