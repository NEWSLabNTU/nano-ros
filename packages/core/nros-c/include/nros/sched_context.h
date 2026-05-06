/**
 * @file sched_context.h
 * @ingroup grp_executor
 * @brief Phase 110.B / 110.C — `SchedContext` API.
 *
 * Register first-class scheduling capabilities on the executor and
 * bind callbacks to them. The runtime dispatches each callback through
 * its SC's bucketed FIFO bitmap or EDF heap (`Priority::COUNT` = 3
 * buckets: Critical / Normal / BestEffort).
 *
 * The default Fifo SC is auto-created at executor init — every
 * callback registered without an explicit
 * `nros_executor_bind_handle_to_sched_context` call binds to it.
 *
 * Single-thread non-preemption: an in-flight `BestEffort` callback
 * blocks `Critical` work that becomes ready mid-cycle. Hard-RT
 * preemption requires the multi-executor model from Phase 110.D.
 */

#ifndef NROS_SCHED_CONTEXT_H
#define NROS_SCHED_CONTEXT_H

/* Type and function definitions live in <nros/nros_generated.h>.
 * This per-module header is a thin shim — nothing here has a
 * hand-written body. */
#include "nros/types.h"

#endif /* NROS_SCHED_CONTEXT_H */
