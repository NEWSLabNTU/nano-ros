/* issue 0680 — per-thread newlib reentrancy for threadx-riscv64.
 *
 * Since issue 0678 this board links the C library its own toolchain ships
 * (newlib), which keeps `errno` in `_impure_ptr` — ONE global pointer to ONE
 * `struct _reent`. picolibc had kept it in compiler TLS, so every thread got
 * its own for free; newlib does not, and nothing pointed that pointer
 * anywhere per-thread. On a board whose whole job is sockets, a failing call
 * on the RX thread was readable as `errno` by the application thread, with no
 * diagnostic on either side.
 *
 * Why the pointer and not something cheaper: `__errno()` here is three
 * instructions returning `_impure_ptr` itself (`_errno` sits at offset 0), so
 * `errno` IS `_impure_ptr->_errno`. Overriding `__errno()` would miss the
 * `_r` entry points — `_write_r`, `_unlink_r` and friends write
 * `ptr->_errno` through the reent they were HANDED, never through
 * `__errno()`. And `-D__DYNAMIC_REENT__` cannot help: this `libc.a` was not
 * built with it, so libc internals keep using `_impure_ptr` regardless of
 * what our headers say. Both shortcuts are the shape that made issue 0679's
 * attempt link and still be wrong — a declaration disagreeing with the
 * archive.
 *
 * So: swap `_impure_ptr` on context switch, which is what ThreadX's and
 * newlib's own documentation describe and what FreeRTOS's
 * `configUSE_NEWLIB_REENTRANT` does.
 */

#include <stddef.h>
#include <sys/lock.h>
#include <sys/reent.h>

#include "tx_api.h"

/* The call sites live in the port assembly (`tx_thread_schedule.S` and the
 * three ISR/return paths) and are compiled out unless the assembler is given
 * `-DTX_ENABLE_EXECUTION_CHANGE_NOTIFY`. Those files include no headers, so
 * the macro reaches them only from the build. If it did not reach THIS
 * translation unit either, the functions below would compile as unreferenced
 * dead code and the swap would silently never happen — the failure mode is
 * silence, so refuse instead. `nano-ros-board-riscv64-qemu.cmake` passes the
 * define to the kernel (assembly) and to the board sources (this file) from
 * adjacent lines for exactly this reason. */
#ifndef TX_ENABLE_EXECUTION_CHANGE_NOTIFY
#error "issue 0680: reent.c needs TX_ENABLE_EXECUTION_CHANGE_NOTIFY, which is \
what turns on the port assembly's execution-notify call sites. Without it the \
per-thread errno swap is never invoked and errno is shared across threads."
#endif

extern TX_THREAD *_tx_thread_current_ptr;

/* Called from `tx_thread_schedule.S` immediately after `_tx_thread_current_ptr`
 * is stored, which is the one point where the incoming thread is known and its
 * first C instruction has not run yet.
 *
 * A thread nros did not create — ThreadX's own system/timer thread, and any
 * application thread built straight on `tx_thread_create` — has a NULL slot
 * and gets newlib's static `_impure_data`. That keeps `_impure_ptr` valid at
 * every instant, which matters because ISR context reads it too. */
VOID _tx_execution_thread_enter(VOID)
{
    TX_THREAD *t = _tx_thread_current_ptr;

    if ((t != TX_NULL) && (t->nros_reent != (struct _reent *) 0)) {
        _impure_ptr = t->nros_reent;
    } else {
        _impure_ptr = &_impure_data;
    }
}

/* The remaining hooks are part of the same port contract and must all exist
 * once the macro is on — the kernel and the port assembly reference FIVE
 * symbols, not the four the call sites in the `.S` files suggest:
 * `_tx_execution_initialize` is called from kernel initialisation, and
 * omitting it is a link error rather than a silent gap (which is how it was
 * found).
 *
 * Nothing here needs them. The enter hook already makes `_impure_ptr` correct
 * for whoever is running, and an ISR borrows the interrupted thread's reent,
 * which is what newlib expects on a single-stack interrupt model. They stay
 * empty rather than absent because the contract is "define these", and a
 * missing one takes the whole image down. */
VOID _tx_execution_initialize(VOID)   { }
VOID _tx_execution_thread_exit(VOID)  { }
VOID _tx_execution_isr_enter(VOID)    { }
VOID _tx_execution_isr_exit(VOID)     { }

/* ===================================================================== *
 * issue 0680, second half — newlib's retargetable locks on TX_MUTEX
 * ===================================================================== *
 *
 * Per-thread `_reent` does not make `malloc` safe. This newlib IS built with
 * `_RETARGETABLE_LOCKING` (its `<sys/lock.h>` declares the hooks and `libc.a`
 * references them 130 times), and it ships DEFAULT implementations that do
 * nothing — so every `malloc`/`free`, `atexit` and stdio-pointer update runs
 * unguarded on a multi-threaded image.
 *
 * Exposure today is narrower than it sounds: issue 0664's `_sbrk` is a bump
 * pointer over a fixed `.heap` with no free, so the failure mode is a torn
 * bump pointer rather than a corrupted free list. A torn bump pointer still
 * hands two threads the same memory.
 *
 * Overriding is all-or-nothing. All ten hooks AND all eight `__lock___*`
 * objects live in ONE archive member, `libc_a-lock.o` (verified with
 * `nm --print-file-name`). Define nine of the eighteen and the linker pulls
 * the member in for the tenth, then reports duplicates for the nine — so the
 * set below is complete on purpose, and must stay complete.
 */

/* `_LOCK_T` is `struct __lock *`; the type is opaque to newlib, so its
 * contents are ours. `created` is not redundant with a NULL check: TX_MUTEX
 * has no "is initialised" state we may read before `tx_mutex_create`. */
struct __lock {
    TX_MUTEX mutex;
    UCHAR created;
};

/* The eight static locks newlib declares `extern` and expects the image to
 * define. Names are newlib's, not ours. */
struct __lock __lock___sinit_recursive_mutex;
struct __lock __lock___sfp_recursive_mutex;
struct __lock __lock___atexit_recursive_mutex;
struct __lock __lock___at_quick_exit_mutex;
struct __lock __lock___malloc_recursive_mutex;
struct __lock __lock___env_recursive_mutex;
struct __lock __lock___tz_mutex;
struct __lock __lock___dd_hash_mutex;
struct __lock __lock___arc4random_mutex;

/* True once the scheduler owns the CPU. Before that — kernel init, and any
 * libc call from `tx_application_define` — there is exactly one flow of
 * control, so a mutex is unnecessary AND unusable: `tx_mutex_get` on a
 * not-yet-running kernel cannot suspend a caller that is not a thread. */
static inline UINT nros_tx_multithreaded(VOID)
{
    return (_tx_thread_current_ptr != TX_NULL) ? 1u : 0u;
}

static VOID nros_lock_ensure(_LOCK_T lock)
{
    if ((lock != (_LOCK_T) 0) && (lock->created == 0u)) {
        /* TX_INHERIT: a low-priority holder of the malloc lock is lifted to
         * the waiter's priority, which is the whole point of using the
         * kernel's mutex rather than a spin flag on a priority-scheduled
         * RTOS. */
        if (tx_mutex_create(&lock->mutex, (CHAR *) "nros_libc", TX_INHERIT) == TX_SUCCESS) {
            lock->created = 1u;
        }
    }
}

VOID __retarget_lock_init(_LOCK_T *lock)             { if (lock != 0) { nros_lock_ensure(*lock); } }
VOID __retarget_lock_init_recursive(_LOCK_T *lock)   { if (lock != 0) { nros_lock_ensure(*lock); } }

VOID __retarget_lock_close(_LOCK_T lock)
{
    if ((lock != (_LOCK_T) 0) && (lock->created != 0u)) {
        (void) tx_mutex_delete(&lock->mutex);
        lock->created = 0u;
    }
}
VOID __retarget_lock_close_recursive(_LOCK_T lock)   { __retarget_lock_close(lock); }

VOID __retarget_lock_acquire(_LOCK_T lock)
{
    if (nros_tx_multithreaded() == 0u) {
        return;
    }
    nros_lock_ensure(lock);
    if ((lock != (_LOCK_T) 0) && (lock->created != 0u)) {
        (void) tx_mutex_get(&lock->mutex, TX_WAIT_FOREVER);
    }
}
VOID __retarget_lock_acquire_recursive(_LOCK_T lock) { __retarget_lock_acquire(lock); }

VOID __retarget_lock_release(_LOCK_T lock)
{
    if (nros_tx_multithreaded() == 0u) {
        return;
    }
    if ((lock != (_LOCK_T) 0) && (lock->created != 0u)) {
        (void) tx_mutex_put(&lock->mutex);
    }
}
VOID __retarget_lock_release_recursive(_LOCK_T lock) { __retarget_lock_release(lock); }

/* newlib's contract is inverted from ThreadX's: non-zero means ACQUIRED. */
int __retarget_lock_try_acquire(_LOCK_T lock)
{
    if (nros_tx_multithreaded() == 0u) {
        return 1;
    }
    nros_lock_ensure(lock);
    if ((lock == (_LOCK_T) 0) || (lock->created == 0u)) {
        return 0;
    }
    return (tx_mutex_get(&lock->mutex, TX_NO_WAIT) == TX_SUCCESS) ? 1 : 0;
}
int __retarget_lock_try_acquire_recursive(_LOCK_T lock) { return __retarget_lock_try_acquire(lock); }

/* ===================================================================== *
 * The hand-maintained TCB offsets the port assembly uses
 * ===================================================================== *
 *
 * `tx_port.h` gives the assembly explicit byte offsets into `TX_THREAD`
 * (`TX_TCB_*_OFF`), and the assembly addresses the control block by them. They
 * are a hand-mirror of a C struct with nothing checking the two agree — the
 * class CLAUDE.md names, where a mirror drifts the moment someone APPENDS to
 * the original.
 *
 * Issue 0680 appends: `TX_THREAD_EXTENSION_3` now carries `nros_reent`. That
 * is SAFE, because every offset above lies in the fixed prologue that precedes
 * `TX_THREAD_EXTENSION_0` — but "is safe" was a fact nobody had checked, and
 * the next append may not be. These assertions are the check, in a TU that
 * sees both the struct and the macros, so the build fails instead of the
 * scheduler writing a stack pointer into the wrong field.
 *
 * Worth having whether or not per-thread reentrancy stays. */
_Static_assert(offsetof(TX_THREAD, tx_thread_id) == TX_TCB_ID_OFF,
               "TX_TCB_ID_OFF disagrees with TX_THREAD — the port assembly is mis-addressing the TCB");
_Static_assert(offsetof(TX_THREAD, tx_thread_run_count) == TX_TCB_RUN_COUNT_OFF,
               "TX_TCB_RUN_COUNT_OFF disagrees with TX_THREAD");
_Static_assert(offsetof(TX_THREAD, tx_thread_stack_ptr) == TX_TCB_STACK_PTR_OFF,
               "TX_TCB_STACK_PTR_OFF disagrees with TX_THREAD — context switch would save the SP elsewhere");
_Static_assert(offsetof(TX_THREAD, tx_thread_stack_start) == TX_TCB_STACK_START_OFF,
               "TX_TCB_STACK_START_OFF disagrees with TX_THREAD");
_Static_assert(offsetof(TX_THREAD, tx_thread_stack_end) == TX_TCB_STACK_END_OFF,
               "TX_TCB_STACK_END_OFF disagrees with TX_THREAD");
_Static_assert(offsetof(TX_THREAD, tx_thread_stack_size) == TX_TCB_STACK_SIZE_OFF,
               "TX_TCB_STACK_SIZE_OFF disagrees with TX_THREAD");
_Static_assert(offsetof(TX_THREAD, tx_thread_time_slice) == TX_TCB_TIME_SLICE_OFF,
               "TX_TCB_TIME_SLICE_OFF disagrees with TX_THREAD");
_Static_assert(offsetof(TX_THREAD, tx_thread_new_time_slice) == TX_TCB_NEW_TIME_SLICE_OFF,
               "TX_TCB_NEW_TIME_SLICE_OFF disagrees with TX_THREAD");
