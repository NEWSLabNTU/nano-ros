/**
 * @file termination.c
 * @brief POSIX termination guard (issue 1732), in its own translation unit.
 *
 * Issue 1776: this TU is separate from `platform.c` on purpose. Every
 * `nros-platform` build with `platform-posix` references the two functions
 * below (`nros_platform::termination`), and a static archive member is linked
 * WHOLE when any symbol in it is needed. While the guard lived in `platform.c`,
 * a reference to it dragged in the entire POSIX platform ABI: the allocator,
 * the clock, tasks and RNG. A link that already carries another port's ABI —
 * `nros-board-threadx` whole-archives the ThreadX one, and `cargo test
 * --workspace` unifies `platform-posix` into the same graph — then failed with
 * twenty duplicate `nros_platform_*` symbols. Here the guard pulls in only
 * itself.
 *
 * Every builder of the POSIX port compiles this file beside `platform.c`
 * (`nros-platform-cffi/build.rs`, `nros-rmw-xrce-cffi/build.rs`,
 * `nros-platform-posix/CMakeLists.txt`). The NuttX build reuses `platform.c`
 * and `net.c` only: it has no `platform-posix` Rust side, so nothing there
 * references the guard.
 */

#define _POSIX_C_SOURCE 200809L

#include <signal.h>
#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

/* Issue 1732 — a termination signal ENDS a hosted image's spin, so the image
 * can close its RMW session on the way out.
 *
 * Without this, SIGTERM/SIGINT killed a native image by the default action:
 * nothing ran, `destroy_session` was never reached, and an XRCE Agent — which
 * has no lease on its clients — kept the dead image's participant, with its
 * topics, services and (since issue 1292) its nodes, until the Agent itself
 * restarted. Zenoh and Cyclone age the leftovers out by lease; XRCE never does.
 *
 * The handler only sets a flag. Tearing a session down inside a signal frame
 * would call into the transport, the allocator and stdio, none of which is
 * async-signal-safe; the boot funnels' spin loops poll
 * `nros_posix_termination_requested()` instead and return, and the funnel then
 * closes the session on its own thread.
 *
 * A SECOND signal ends the image by the default action, as an unguarded process
 * would have ended on the first: an image wedged in its teardown must still be
 * stoppable without SIGKILL. (The same contract issue 1741 gave threadx-linux,
 * whose guard is a sigwait thread because its scheduler owns the signal mask.)
 *
 * Installed only over the DEFAULT disposition. A handler the application
 * installed is its own, and SIG_IGN (`nohup`) is the operator's; both are left
 * alone, so this never changes what a program that already handles the signal
 * does. Returns how many of SIGTERM/SIGINT carry the guard; idempotent. */
static volatile sig_atomic_t s_termination_requested = 0;

/* phase-521 W6 (RFC-0113 D2) — the wake objects a termination posts.
 *
 * A hosted spin loop polls `nros_posix_termination_requested()` between
 * spins, and a spin now parks until the tier's next deadline instead of for a
 * fixed 10 ms. A flag alone would be seen only when that park ends, so the
 * handler also posts every live wake object: each parked executor returns,
 * sees the flag and closes its session.
 *
 * `platform.c` registers each wake object it initialises, with the function
 * that posts it, and removes it before destroying it. The registry holds a
 * function pointer rather than naming `nros_platform_wake_signal`, so this
 * translation unit references nothing in `platform.c` (issue 1776). A wake
 * object that finds every slot taken is not registered: its executor sees a
 * termination only when its park ends, which is the behaviour every executor
 * had before. */
#define NROS_POSIX_TERMINATION_WAKES 32

typedef int8_t (*nros_posix_wake_post_fn)(void *w);

static _Atomic(void *) s_wake_slot[NROS_POSIX_TERMINATION_WAKES];
static _Atomic(nros_posix_wake_post_fn) s_wake_post;
/* Handlers inside the post loop right now; a removal waits for zero so it
 * never returns while a handler may still post the object it removed. */
static atomic_int s_wake_posting;

int nros_posix_termination_wake_add(nros_posix_wake_post_fn post, void *w);
int nros_posix_termination_wake_add(nros_posix_wake_post_fn post, void *w) {
    if (post == NULL || w == NULL) {
        return -1;
    }
    atomic_store(&s_wake_post, post);
    for (size_t i = 0; i < NROS_POSIX_TERMINATION_WAKES; i++) {
        void *expected = NULL;
        if (atomic_compare_exchange_strong(&s_wake_slot[i], &expected, w)) {
            return 0;
        }
    }
    return -1;
}

void nros_posix_termination_wake_remove(void *w);
void nros_posix_termination_wake_remove(void *w) {
    if (w == NULL) {
        return;
    }
    for (size_t i = 0; i < NROS_POSIX_TERMINATION_WAKES; i++) {
        void *expected = w;
        if (atomic_compare_exchange_strong(&s_wake_slot[i], &expected, NULL)) {
            break;
        }
    }
    while (atomic_load(&s_wake_posting) != 0) {
        /* A handler on another thread is mid-post; it holds no lock and runs
         * a bounded loop, so this ends. */
    }
}

static void nros_posix_post_wakes(void) {
    nros_posix_wake_post_fn post = atomic_load(&s_wake_post);
    if (post == NULL) {
        return;
    }
    atomic_fetch_add(&s_wake_posting, 1);
    for (size_t i = 0; i < NROS_POSIX_TERMINATION_WAKES; i++) {
        void *w = atomic_load(&s_wake_slot[i]);
        if (w != NULL) {
            (void) post(w);
        }
    }
    atomic_fetch_sub(&s_wake_posting, 1);
}

static void nros_posix_on_termination(int sig) {
    if (s_termination_requested) {
        /* Both async-signal-safe (POSIX.1-2008 §2.4.3). The signal is masked
         * while its handler runs, so `raise` takes effect on return. */
        (void) signal(sig, SIG_DFL);
        (void) raise(sig);
        return;
    }
    s_termination_requested = 1;
    /* `sem_post` and `write` are async-signal-safe; lock-free atomics are. */
    nros_posix_post_wakes();
}

int nros_posix_install_termination_guard(void);
int nros_posix_install_termination_guard(void) {
    static const int sigs[2] = {SIGTERM, SIGINT};
    int guarded = 0;
    for (size_t i = 0; i < sizeof(sigs) / sizeof(sigs[0]); i++) {
        struct sigaction cur;
        if (sigaction(sigs[i], NULL, &cur) != 0) {
            continue;
        }
        if ((cur.sa_flags & SA_SIGINFO) == 0 && cur.sa_handler == nros_posix_on_termination) {
            guarded++;
            continue;
        }
        if ((cur.sa_flags & SA_SIGINFO) != 0 || cur.sa_handler != SIG_DFL) {
            continue;
        }
        struct sigaction sa;
        memset(&sa, 0, sizeof(sa));
        sa.sa_handler = nros_posix_on_termination;
        sigemptyset(&sa.sa_mask);
        /* SA_RESTART: a transport blocked in recv/poll keeps its own timeout
         * rather than failing EINTR into a spurious spin error. Every spin step
         * is bounded, so the flag is seen within one step anyway. */
        sa.sa_flags = SA_RESTART;
        if (sigaction(sigs[i], &sa, NULL) == 0) {
            guarded++;
        }
    }
    return guarded;
}

int nros_posix_termination_requested(void);
int nros_posix_termination_requested(void) {
    return s_termination_requested != 0;
}
