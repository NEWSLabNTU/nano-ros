/// @file main.c
/// @brief issue 0680 — prove `errno` is PER-THREAD on threadx-riscv64.
///
/// Since issue 0678 this board links its toolchain's newlib, whose `errno`
/// resolves through `_impure_ptr` — one global pointer to one `struct _reent`.
/// Issue 0680 gives each task its own and swaps the pointer on context switch.
///
/// Nothing in the suite could tell that fix from its absence: `errno` is read
/// on error paths, and every other test asserts delivery. This fixture is the
/// discriminator, and it is written to FAIL LOUDLY on the unfixed board rather
/// than to pass quietly on the fixed one.
///
/// Shape, and why each part is needed:
///
///   * BOTH threads are spawned through `nros_platform_task_init`. That is the
///     path that allocates the reent, so a thread created with a bare
///     `tx_thread_create` would carry a NULL slot, fall back to the shared
///     `_impure_data`, and fail this test even WITH the fix. Testing the
///     platform ABI is the point, not a ThreadX detail.
///   * The handoff is explicit. `victim` sets `errno`, then hands over; only
///     then does `observer` read its own. Without the ordering a pass could
///     mean "the write had not happened yet" — a race that reports success,
///     which is the failure mode this whole issue is about.
///   * `observer` writes its `errno` FIRST and re-reads it after the victim
///     ran. Checking only "is it still 0" would pass on a board where the
///     write silently went nowhere.

#include <errno.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <nros/app_main.h>
#include <nros/platform.h>

/* Markers. `nros_tests::output` greps these; keep them in lockstep with
 * `ERRNO_ISOLATION_*` there. */
#define MARK_PASS "errno-isolation: verdict PASS per-thread errno"
#define MARK_FAIL "errno-isolation: verdict FAIL shared errno"
#define MARK_SETUP "errno-isolation: verdict SETUP failed"

/* Hand-off state. `volatile` because the two tasks run on one core with a
 * cooperative-looking handoff and nothing here is a memory barrier; the
 * scheduler is the synchronisation. */
static volatile int observer_ready = 0;
static volatile int victim_ran = 0;
static volatile int observer_first_errno = -1;
static volatile int observer_second_errno = -1;
static volatile int victim_errno = -1;
static volatile int observer_done = 0;

/* Storage for the two tasks: the port declares its own size. */
static unsigned char victim_task[512] __attribute__((aligned(16)));
static unsigned char observer_task[512] __attribute__((aligned(16)));

/* Sets `errno` through a REAL failing libc call, not by assignment.
 *
 * `strtol` overflow sets ERANGE from inside `_strtol_r`, which stores through
 * the reent pointer it was HANDED — the exact path an `__errno()`
 * interposition would miss, and the reason issue 0680 swaps `_impure_ptr`
 * rather than overriding the accessor. Assigning `errno = ERANGE` here would
 * test the declaration and not the library.
 *
 * NOT `write(-1, …)`, which the issue proposed and which cannot work on this
 * board: `_write` in `startup.c` is `(void)fd;` followed by an unconditional
 * UART loop returning `len`, so a write to a closed descriptor SUCCEEDS. That
 * was found by running this fixture, which is the argument for having it.
 * `strtol` needs no syscall stub at all, so it cannot be defeated the same
 * way. */
static void *victim_entry(void *arg) {
    (void) arg;
    printf("errno-isolation: victim entered\n");

    /* WAIT for the observer to have claimed and parked. Priority alone does
     * NOT order these: the first run of this fixture had the victim finish
     * before the observer ever entered, so the observer's own `errno = 0`
     * overwrote the shared value it was there to detect — and the test passed
     * on an UNFIXED board. A pass that survives the fix being removed is worth
     * nothing, so the ordering is now enforced rather than assumed. */
    for (int i = 0; (observer_ready == 0) && (i < 200); i++) {
        nros_platform_sleep_us(10000u);
    }
    if (observer_ready == 0) {
        printf("%s (observer never parked)\n", MARK_SETUP);
        victim_ran = 1;
        return NULL;
    }

    errno = 0;
    (void) strtol("99999999999999999999999999", NULL, 10);
    victim_errno = errno;
    victim_ran = 1;
    return NULL;
}

static void *observer_entry(void *arg) {
    (void) arg;
    printf("errno-isolation: observer entered\n");
    /* Claim a distinct value first, so "unchanged" is a real observation
     * rather than the absence of any write at all. */
    errno = 0;
    observer_first_errno = errno;
    observer_ready = 1;

    /* Let the victim run. Lower priority than the victim, so yielding is
     * enough; the bound stops a broken board hanging the image. */
    /* Bounds are in TICKS, not microseconds: `nros_platform_sleep_us` rounds
     * up to ThreadX's tick (10 ms here), so a 1 ms request costs 10 ms and a
     * naive 1000-iteration bound is ten seconds of wall clock. Sized for a
     * couple of seconds. */
    for (int i = 0; (victim_ran == 0) && (i < 200); i++) {
        nros_platform_sleep_us(10000u);
    }
    observer_second_errno = errno;
    observer_done = 1;
    return NULL;
}

int nros_app_main(int argc, char **argv) {
    (void) argc;
    (void) argv;
    printf("errno-isolation: start\n");

    /* The port declares its own storage size; a hardcoded buffer that is too
     * small lets `tx_thread_create` scribble past it, and the symptom is a
     * hang with no output — which is what the first run of this fixture did.
     * Check instead of assuming. */
    size_t need = nros_platform_task_storage_size();
    printf("errno-isolation: task storage need=%u have=%u\n",
           (unsigned) need, (unsigned) sizeof(victim_task));
    if (need > sizeof(victim_task)) {
        printf("%s (task storage %u > %u)\n", MARK_SETUP,
               (unsigned) need, (unsigned) sizeof(victim_task));
        return 1;
    }

    nros_platform_task_attr_t attr;
    nros_platform_task_attr_init(&attr);
    attr.stack_bytes = 4096u;

    /* Observer first and at LOWER urgency, so it is already parked in its
     * wait loop when the victim runs. */
    attr.priority = NROS_PLATFORM_PRIORITY_MAX / 4;
    if (nros_platform_task_init(observer_task, &attr, observer_entry, NULL) != 0) {
        printf("%s (observer)\n", MARK_SETUP);
        return 1;
    }
    printf("errno-isolation: observer spawned\n");
    attr.priority = NROS_PLATFORM_PRIORITY_MAX / 2;
    if (nros_platform_task_init(victim_task, &attr, victim_entry, NULL) != 0) {
        printf("%s (victim)\n", MARK_SETUP);
        return 1;
    }

    printf("errno-isolation: victim spawned\n");
    /* Bounded wait on the tasks' own flags rather than `task_join`. ThreadX
     * has no native join, so the port polls thread state — and a fixture that
     * HANGS reports nothing at all, which is the one outcome worse than a
     * failure here. */
    for (int i = 0; ((victim_ran == 0) || (observer_done == 0)) && (i < 300); i++) {
        nros_platform_sleep_us(10000u);
    }
    if ((victim_ran == 0) || (observer_done == 0)) {
        printf("%s (tasks did not finish: victim_ran=%d observer_done=%d)\n",
               MARK_SETUP, victim_ran, observer_done);
        return 1;
    }

    printf("errno-isolation: victim errno=%d observer before=%d after=%d\n",
           victim_errno, observer_first_errno, observer_second_errno);

    /* The victim must actually have failed. A board where `write(-1, …)`
     * succeeded, or never ran, proves nothing either way — say so instead of
     * reporting a pass. */
    if ((victim_ran == 0) || (victim_errno == 0)) {
        printf("%s (victim errno=%d ran=%d — the probe did not fire)\n",
               MARK_SETUP, victim_errno, victim_ran);
        return 1;
    }

    if (observer_second_errno == victim_errno) {
        printf("%s (observer saw the victim's errno=%d)\n", MARK_FAIL, victim_errno);
        return 1;
    }
    if (observer_second_errno != 0) {
        printf("%s (observer errno=%d, expected 0)\n", MARK_FAIL, observer_second_errno);
        return 1;
    }

    printf("%s\n", MARK_PASS);
    return 0;
}

NROS_APP_MAIN_REGISTER()
