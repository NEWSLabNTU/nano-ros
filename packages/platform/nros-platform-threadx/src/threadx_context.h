/*
 * Issue 1750 -- "only a ThreadX context calls a ThreadX service", enforced at
 * the platform seam.
 *
 * On a bare-metal ThreadX port every thread in the image is a ThreadX thread,
 * so this header compiles to nothing there. The hosted Linux port
 * (threadx-linux) is different: ThreadX is one host process, and a HOST
 * library linked into it -- Cyclone's ddsrt is the measured one -- runs
 * threads of its own that ThreadX never created. Those threads reach the
 * platform ABI (ddsrt's heap is funnelled into `nros_platform_alloc`, issue
 * 0832; Cyclone's data-available listener reaches `nros_platform_wake_signal`).
 *
 * A ThreadX service called from such a thread is not merely unsupported, it
 * WEDGES THE KERNEL. Every service brackets itself with TX_DISABLE/TX_RESTORE,
 * which this port implements in `_tx_thread_interrupt_control()` as a lock of
 * the recursive `_tx_linux_mutex` -- and it unlocks again only for an ISR or
 * for the thread ThreadX believes is current. A foreign thread is neither, so
 * each call leaks one recursion level. Measured on the C service-server over
 * Cyclone: `_tx_linux_mutex.__count` 1508 held by Cyclone's `tev` thread
 * within 5 s, scheduler, timer ISR and Cyclone's `dq.builtins` all parked on
 * it, no request ever served.
 *
 * So every ThreadX-calling entry point of this port asks
 * `nros_threadx_in_kernel_context()` first, and a foreign caller gets:
 *
 *   - the heap: a host-heap allocation, and a deferred release for a pool
 *     block it frees (platform.c, "Foreign host threads and the heap");
 *   - the clock: a direct read of the tick counter, which needs no lock;
 *   - a wake / condvar SIGNAL: a refusal (-1) and one line on stderr -- a lost
 *     wake costs the waiter its bounded timeout, never liveness;
 *   - anything else (mutex, wait, sleep, task, critical section, timer): a
 *     FATAL refusal. There is no correct answer to "lock a ThreadX mutex from
 *     a thread ThreadX cannot suspend", and carrying on unlocked is a data race
 *     that reads like a working image.
 */
#ifndef NROS_PLATFORM_THREADX_CONTEXT_H
#define NROS_PLATFORM_THREADX_CONTEXT_H

#include <tx_api.h>

/* The hosted Linux port, identified by the port's OWN mechanism rather than by
 * the host OS: `tx_linux_mutex_lock` is the macro its TX_DISABLE is built on,
 * defined unconditionally by `ports/linux/gnu/inc/tx_port.h`. A `__linux__`
 * test would answer a different question (which host compiled this), and the
 * thread-identity flag read below exists only in this port. */
#if defined(tx_linux_mutex_lock)
#  define NROS_THREADX_HOSTED_PORT 1
#else
#  define NROS_THREADX_HOSTED_PORT 0
#endif

#if NROS_THREADX_HOSTED_PORT

/* 1 when the calling thread may call a ThreadX service: a thread ThreadX
 * created, the port's timer-ISR thread, or the boot thread while
 * `tx_application_define` runs. 0 for every other host thread. */
int nros_threadx_in_kernel_context(void);

/* A foreign caller reached `fn`. `fatal` != 0 ends the process (with a line on
 * stderr naming `fn` and this issue); otherwise one line is printed per
 * process and the caller returns its refusal. */
void nros_threadx_refuse_foreign(const char *fn, int fatal);

/* The guard every ThreadX-calling entry point opens with. */
#  define NROS_THREADX_KERNEL_ONLY_FATAL()                                                         \
      do {                                                                                         \
          if (!nros_threadx_in_kernel_context()) {                                                 \
              nros_threadx_refuse_foreign(__func__, 1);                                            \
          }                                                                                        \
      } while (0)
#  define NROS_THREADX_KERNEL_ONLY_OR_RETURN(ret)                                                  \
      do {                                                                                         \
          if (!nros_threadx_in_kernel_context()) {                                                 \
              nros_threadx_refuse_foreign(__func__, 0);                                            \
              return (ret);                                                                        \
          }                                                                                        \
      } while (0)

#else /* bare-metal ports: every thread is a ThreadX thread */

#  define NROS_THREADX_KERNEL_ONLY_FATAL()          ((void) 0)
#  define NROS_THREADX_KERNEL_ONLY_OR_RETURN(ret)   ((void) 0)

#endif /* NROS_THREADX_HOSTED_PORT */

#endif /* NROS_PLATFORM_THREADX_CONTEXT_H */
