/*
 * Copyright(c) 2026 ZettaScale Technology and others
 *
 * SPDX-License-Identifier: EPL-2.0 OR BSD-3-Clause
 */
#ifndef DDSRT_TIME_THREADX_H
#define DDSRT_TIME_THREADX_H

#include <assert.h>
#include <tx_api.h>

#if defined (__cplusplus)
extern "C" {
#endif

#ifndef TX_TIMER_TICKS_PER_SECOND
#define TX_TIMER_TICKS_PER_SECOND 100u
#endif

#define DDSRT_NSECS_PER_TICK (DDS_NSECS_IN_SEC / TX_TIMER_TICKS_PER_SECOND)

inline ULONG
ddsrt_duration_to_ticks_ceil(
  dds_duration_t reltime)
{
  ULONG ticks = 0;

  assert(TX_WAIT_FOREVER > TX_TIMER_TICKS_PER_SECOND);

  if (reltime == DDS_INFINITY) {
    ticks = TX_WAIT_FOREVER;
  } else if (reltime > 0) {
    dds_duration_t max_nsecs =
      (DDS_INFINITY / DDSRT_NSECS_PER_TICK < TX_WAIT_FOREVER
        ? DDS_INFINITY - 1 : TX_WAIT_FOREVER * DDSRT_NSECS_PER_TICK);

    if (reltime > max_nsecs - (DDSRT_NSECS_PER_TICK - 1)) {
      ticks = TX_WAIT_FOREVER;
    } else {
      ticks = (ULONG)((reltime + (DDSRT_NSECS_PER_TICK - 1)) / DDSRT_NSECS_PER_TICK);
    }
  }

  return ticks;
}

#if defined (__cplusplus)
}
#endif

#endif /* DDSRT_TIME_THREADX_H */
