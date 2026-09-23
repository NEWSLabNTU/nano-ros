/*
 * phase-460 W6 -- the Zephyr stack-slot pool releases what it claims.
 *
 * WHAT IS UNDER TEST. `zephyr/nros_platform_zephyr_shims.c` owns the entire
 * task pool of the Zephyr platform: `nros_claim_thread_slot` is the only path
 * to a thread, `nros_zephyr_task_slot_release` is the only path back, and
 * `_z_task_join` in `zephyr/nros_zenoh_zephyr_system.c` is the only caller of
 * the release. The counter that pairing replaced only ever ROSE, so a slot was
 * spent for the life of the image even after its task had exited; zenoh-pico
 * reconnects on every lease expiry, so the action image walked out of slots at
 * the third reconnect and could not reopen at all (issue 0839).
 *
 * That fix has been in the tree since 0839 with NOTHING asserting it. It is
 * not reachable from any host test today and its failure mode on target is
 * silent for minutes -- the image comes up, declares every entity, transmits,
 * and simply never receives -- so a regression would be found the way 0839 was
 * found, from a session-expiry log. This file is the gate the fix never got.
 *
 * WHY IT CAN RUN ON THE HOST. The pool is plain C: a table, a mutex and
 * pthread_create/join. Everything Zephyr-specific around it is preprocessed
 * away by the compile line, and what remains is backed by the host's own
 * pthreads through the stub tree in `tests/zephyr/host-shims/`. So the
 * REAL shims source is compiled and linked here -- not a copy of the
 * algorithm, which would assert only that two files agree.
 *
 * NROS_ZEPHYR_MAX_THREADS is set on the compile line and this file reads the
 * same macro, so "N" below is the pool's real capacity in this build and the
 * runner exercises more than one value of it.
 *
 * THE TWO CASES, both driven by `tests/zephyr/run-thread-slots.sh`:
 *
 *   n-plus-2  Claim every slot at once, prove the pool is full at exactly N
 *             (so N is measured here, not assumed), join all N, then create
 *             two MORE and refill the whole pool. A shim that leaks a slot on
 *             join fails at the first of the two extra creates; one that
 *             releases only some of them fails at the refill.
 *
 *   detach    The documented teardown that does NOT release. One task is
 *             detached instead of joined, so its slot is spent; exactly N-1
 *             creates must then succeed and the next must refuse, naming the
 *             knob. This is the negative control: it fails if a shim releases
 *             on exit rather than on join, which would hand a live thread's
 *             stack to the next task.
 *
 * The cases are separate PROCESSES because the slot table is file-scope state
 * and the detach case spends a slot permanently.
 */

#include <errno.h>
#include <pthread.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <zephyr/sys/printk.h>

#include "thread_slot_shim_decls.h"

#ifndef NROS_ZEPHYR_MAX_THREADS
#error "the runner must set NROS_ZEPHYR_MAX_THREADS for the test and the shims alike"
#endif

#define POOL NROS_ZEPHYR_MAX_THREADS

/* printk is the shims' refusal channel (deliberately printk and not LOG_ERR:
 * it runs before any log backend is guaranteed up). Route it to stdout so the
 * runner can read the refusal line, and count it here so the assertions do not
 * have to re-parse the output. */
static int g_refusals;

void printk(const char* fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    (void)vprintf(fmt, ap);
    va_end(ap);
    if (strstr(fmt, "OUT OF THREAD SLOTS") != NULL) {
        g_refusals++;
    }
    (void)fflush(stdout);
}

/* Every worker parks until the test lets it finish, so a task that has been
 * created is genuinely OCCUPYING its slot while the next create is attempted.
 * A worker that merely returned would make the test pass on a shim that
 * released on exit, which is the bug the release-on-join rule exists to
 * prevent. */
static pthread_mutex_t g_gate_lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t g_gate_cv = PTHREAD_COND_INITIALIZER;
static bool g_gate_open;

static void* worker(void* arg) {
    (void)arg;
    (void)pthread_mutex_lock(&g_gate_lock);
    while (!g_gate_open) {
        (void)pthread_cond_wait(&g_gate_cv, &g_gate_lock);
    }
    (void)pthread_mutex_unlock(&g_gate_lock);
    return NULL;
}

static void open_the_gate(void) {
    (void)pthread_mutex_lock(&g_gate_lock);
    g_gate_open = true;
    (void)pthread_cond_broadcast(&g_gate_cv);
    (void)pthread_mutex_unlock(&g_gate_lock);
}

static void close_the_gate(void) {
    (void)pthread_mutex_lock(&g_gate_lock);
    g_gate_open = false;
    (void)pthread_mutex_unlock(&g_gate_lock);
}

static int g_failures;

static void check(bool ok, const char* what) {
    if (ok) {
        printf("  ok    %s\n", what);
    } else {
        printf("  FAIL  %s\n", what);
        g_failures++;
    }
}

/* Join and release, in that order and never the other way: the join RETURNING
 * is the proof the thread is gone, so its stack can be handed out again. */
static void join_and_release(pthread_t t) {
    int rc = pthread_join(t, NULL);
    if (rc != 0) {
        printf("  FAIL  pthread_join: %s\n", strerror(rc));
        g_failures++;
        return;
    }
    nros_zephyr_task_slot_release(t);
}

/* ---- case: n-plus-2 ----------------------------------------------------- */

static void case_n_plus_2(void) {
    pthread_t held[POOL];

    printf("case n-plus-2 (pool capacity N=%d)\n", POOL);
    close_the_gate();

    int created = 0;
    for (int i = 0; i < POOL; i++) {
        if (nros_zephyr_task_create(&held[i], worker, NULL) == 0) {
            created++;
        } else {
            break;
        }
    }
    check(created == POOL, "every one of the N slots can be claimed at once");

    /* The pool is full, so this must refuse. Without it the N+2 below would
     * also pass on a pool that is secretly larger than N, which would prove
     * nothing about release. */
    pthread_t overflow;
    g_refusals = 0;
    int over_rc = nros_zephyr_task_create(&overflow, worker, NULL);
    check(over_rc != 0, "the create past a full pool is refused");
    check(g_refusals == 1, "the refusal names OUT OF THREAD SLOTS");

    /* Release every slot the documented way. */
    open_the_gate();
    for (int i = 0; i < created; i++) {
        join_and_release(held[i]);
    }
    close_the_gate();

    /* THE GATE. A shim that leaks a slot on join fails at the FIRST of these:
     * the pool was proven full at N above, so a create can only succeed now if
     * the joins gave their slots back.
     *
     * One at a time, each joined and released before the next, because the
     * pool's floor is a capacity of ONE (issue 1015) and the sweep includes
     * it. Holding both at once would assert something about N >= 2 rather than
     * about release, and the refill below asserts the stronger property
     * anyway. */
    int extras = 0;
    for (int i = 0; i < 2; i++) {
        pthread_t t;
        if (nros_zephyr_task_create(&t, worker, NULL) != 0) {
            break;
        }
        extras++;
        open_the_gate();
        join_and_release(t);
        close_the_gate();
    }
    check(extras == 2, "two MORE tasks are created after the N joins (slots released)");

    /* ALL N came back, not just one. A shim that released a single slot per
     * join-storm -- or matched the wrong owner and freed one slot twice --
     * satisfies the two creates above and fails here. */
    int refill = 0;
    for (int i = 0; i < POOL; i++) {
        if (nros_zephyr_task_create(&held[i], worker, NULL) != 0) {
            break;
        }
        refill++;
    }
    check(refill == POOL, "the whole pool refills: every one of the N slots was released");

    open_the_gate();
    for (int i = 0; i < refill; i++) {
        join_and_release(held[i]);
    }
    close_the_gate();
}

/* ---- case: detach (negative control) ------------------------------------ */

static void case_detach(void) {
    pthread_t held[POOL];

    printf("case detach (pool capacity N=%d)\n", POOL);
    close_the_gate();

    int created = 0;
    for (int i = 0; i < POOL; i++) {
        if (nros_zephyr_task_create(&held[i], worker, NULL) == 0) {
            created++;
        } else {
            break;
        }
    }
    check(created == POOL, "every one of the N slots can be claimed at once");

    /* The teardown that must NOT release. `_z_task_detach` detaches and stops
     * there on purpose: a detached thread may still be running, and handing
     * its stack to the next task is a worse bug than the leak. */
    int detach_rc = pthread_detach(held[0]);
    check(detach_rc == 0, "one task is detached rather than joined");

    open_the_gate();
    for (int i = 1; i < created; i++) {
        join_and_release(held[i]);
    }
    close_the_gate();

    /* N-1 slots came back, one did not. */
    pthread_t again[POOL];
    g_refusals = 0;
    int again_n = 0;
    for (int i = 0; i < POOL; i++) {
        if (nros_zephyr_task_create(&again[i], worker, NULL) != 0) {
            break;
        }
        again_n++;
    }
    check(again_n == POOL - 1, "exactly N-1 slots are reusable; the detached one is not");
    check(g_refusals == 1, "the create past the detached slot refuses, naming the knob");

    open_the_gate();
    for (int i = 0; i < again_n; i++) {
        join_and_release(again[i]);
    }
    close_the_gate();
}

int main(int argc, char** argv) {
    const char* which = (argc > 1) ? argv[1] : "";

    if (strcmp(which, "n-plus-2") == 0) {
        case_n_plus_2();
    } else if (strcmp(which, "detach") == 0) {
        case_detach();
    } else {
        fprintf(stderr, "usage: %s n-plus-2|detach\n", argv[0]);
        return 2;
    }

    if (g_failures != 0) {
        printf("%d assertion(s) FAILED in case `%s`\n", g_failures, which);
        return 1;
    }
    printf("case `%s` passed\n", which);
    return 0;
}
