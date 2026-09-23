/*
 * phase-460 W7 / issue 1425 -- an exhausted platform heap reaches the FATAL
 * HOOK, not only a printk.
 *
 * WHAT IS UNDER TEST. `nros_platform_alloc` in
 * `packages/platform/nros-platform-zephyr/src/platform.c` used to print a
 * "HEAP EXHAUSTED" line and return NULL. A returned NULL is only ever handled
 * by code that was WRITTEN to handle it, and the board class this port exists
 * for -- the MR-CANHUBK344, whose console UART is not wired and whose second
 * UART carries the zenoh serial transport -- has nowhere for the line to go. So
 * the observable outcome of an exhausted heap was an image that kept running
 * and quietly did less.
 *
 * This drives the REAL `platform.c`, compiled for the host against the stub
 * Zephyr tree in `tests/zephyr/host-platform/`, with a fake arena that refuses.
 * `k_panic()` is the seam: on a board it enters Zephyr's fatal path so the
 * image's `k_sys_fatal_error_handler` runs (RFC-0077), and here it longjmps
 * back so the test can say it was reached.
 *
 * WHY NOT native_sim, which the phase doc names. A native_sim run needs a
 * provisioned Zephyr workspace and `west`, which puts it beside `run-c.sh` in
 * the suite no gate lane can reach -- and the phase's own gate table already
 * says W7's native_sim half does not run in the fast tier. What it would add
 * over this is the kernel's real fatal path, and what it would cost is a gate
 * that never runs. This asserts the same chain, on `cc` and pthreads, on the
 * lane `just ci l1` actually executes. Same trade, and same stub-tree
 * technique, as phase-460 W6's thread-slot test.
 *
 * THREE CASES, run by `tests/zephyr/run-heap-exhaustion.sh`:
 *
 *   fatal      with CONFIG_NROS_HEAP_EXHAUSTION_IS_FATAL: the record is
 *              written, the line is printed, and the hook is ENTERED -- in
 *              that order, because the record has to survive the halt.
 *   not-fatal  without it: NULL comes back, the record is still written, the
 *              line is still printed, and the hook is NOT entered. The knob's
 *              documented off-behaviour, and the wave's negative control.
 *   ok         a request the arena satisfies touches none of it.
 *
 * The runner also runs `fatal` against the NOT-fatal binary and requires it to
 * FAIL, which is what measures this gate's teeth rather than claiming them.
 */

#include <setjmp.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <nros/platform.h>

/* ---- the observations ---------------------------------------------------
 *
 * A SEQUENCE, not three booleans. "the record was written" and "the hook was
 * entered" are both true of an implementation that panics first and writes
 * afterwards, which is the one implementation that cannot work: the record is
 * what a halted board is read through, so a write after the halt never
 * happens. Recording the order is what makes that assertable. */

enum event {
    EV_RECORD,  /* nros_boot_report_note_heap_alloc_failed */
    EV_LINE,    /* the HEAP EXHAUSTED printk */
    EV_HOOK,    /* k_panic, i.e. the image's fatal handler on a board */
};

#define MAX_EVENTS 16
static enum event events[MAX_EVENTS];
static size_t event_count;

static void record_event(enum event e) {
    if (event_count < MAX_EVENTS) {
        events[event_count++] = e;
    }
}

static bool saw(enum event e) {
    for (size_t i = 0; i < event_count; i++) {
        if (events[i] == e) return true;
    }
    return false;
}

static long index_of(enum event e) {
    for (size_t i = 0; i < event_count; i++) {
        if (events[i] == e) return (long) i;
    }
    return -1;
}

static size_t recorded_failed_size;
static size_t recorded_heap_peak;
static size_t recorded_heap_capacity;

/* Every printk this run emitted, concatenated. The test asserts on the text a
 * reader would actually see rather than on the format string. */
#define LOG_CAP 4096
static char log_buf[LOG_CAP];
static size_t log_len;

void printk(const char* fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(log_buf + log_len, LOG_CAP - log_len, fmt, ap);
    va_end(ap);
    if (n > 0) {
        log_len += (size_t) n;
        if (log_len >= LOG_CAP) log_len = LOG_CAP - 1;
    }
    if (strstr(log_buf, "HEAP EXHAUSTED") != NULL && !saw(EV_LINE)) {
        record_event(EV_LINE);
    }
}

/* ---- the fatal seam -----------------------------------------------------
 *
 * On a board `k_panic()` never returns and the fatal handler runs. Here it
 * records that it was reached and unwinds to the case driver, which is the
 * only way a host process can observe "the image stopped here" and still
 * report it. */
static jmp_buf panic_landing;

void k_panic(void) {
    record_event(EV_HOOK);
    longjmp(panic_landing, 1);
}

/* ---- the arena `platform.c` allocates from ------------------------------
 *
 * `nros_zephyr_heap_*` are the Rust half of the image on a board. Here they are
 * a fake that refuses anything over `heap_limit`, which is the whole point:
 * exhausting a real rlsf arena would need to allocate megabytes to prove a
 * property that is about what happens on the REFUSAL. */
static size_t heap_limit = 64;
static size_t heap_capacity_bytes = 94208;
static size_t heap_peak_bytes = 18352;
static char scratch[64];

void* nros_zephyr_heap_alloc(size_t size) {
    return size <= heap_limit ? scratch : NULL;
}

void* nros_zephyr_heap_realloc(void* ptr, size_t size) {
    (void) ptr;
    return size <= heap_limit ? scratch : NULL;
}

void nros_zephyr_heap_free(void* ptr) {
    (void) ptr;
}

size_t nros_zephyr_heap_capacity(void) {
    return heap_capacity_bytes;
}

size_t nros_zephyr_heap_used(void) {
    return 0;
}

size_t nros_zephyr_heap_peak(void) {
    return heap_peak_bytes;
}

/* ---- entropy ------------------------------------------------------------
 *
 * `platform.c` exports six random entry points that call these, and a host
 * LINK needs them even though nothing on the allocation path does. Aborting
 * rather than returning a number, on the stub tree's rule: a test that drifted
 * into reading entropy from this file would be reading something this file
 * never implemented. */
uint32_t sys_rand32_get(void) {
    fprintf(stderr, "host-platform: sys_rand32_get() is a STUB and was called.\n");
    abort();
}

void sys_rand_get(void* dst, size_t len) {
    (void) dst;
    (void) len;
    fprintf(stderr, "host-platform: sys_rand_get() is a STUB and was called.\n");
    abort();
}

/* ---- the boot report ----------------------------------------------------
 *
 * The Rust `nros-node` half on a board. Stood in for here so the test can say
 * WHAT was written and WHEN, which is what `read-boot-report.py` will be asked
 * for off a halted board. */
void nros_boot_report_note_heap(size_t peak, size_t capacity) {
    recorded_heap_peak = peak;
    recorded_heap_capacity = capacity;
}

void nros_boot_report_note_heap_alloc_failed(size_t size) {
    recorded_failed_size = size;
    record_event(EV_RECORD);
}

/* ---- the harness --------------------------------------------------------- */

static int failures;

static void check(bool ok, const char* what) {
    if (ok) {
        printf("  ok    %s\n", what);
    } else {
        printf("  FAIL  %s\n", what);
        failures++;
    }
}

static void reset(void) {
    event_count = 0;
    log_len = 0;
    log_buf[0] = '\0';
    recorded_failed_size = 0;
    recorded_heap_peak = 0;
    recorded_heap_capacity = 0;
}

/* The request that cannot fit. 427968 is not a round number: it is the size
 * three zephyr xrce-cpp cells died on in issue 0968, and using it keeps the
 * number in the message something a reader can recognise. */
#define TOO_BIG 427968u

static void case_fatal(void) {
    reset();
    void* p = (void*) 1;
    if (setjmp(panic_landing) == 0) {
        p = nros_platform_alloc(TOO_BIG);
        /* Reached only when the hook did NOT fire. */
        check(false, "nros_platform_alloc RETURNED; the fatal hook was not entered");
        (void) p;
    }
    check(saw(EV_HOOK), "the fatal hook was entered");
    check(saw(EV_RECORD), "the failed request reached the boot report");
    check(recorded_failed_size == TOO_BIG,
          "the boot report names the size that did not fit");
    check(saw(EV_LINE), "the HEAP EXHAUSTED line was still printed");
    check(index_of(EV_RECORD) < index_of(EV_HOOK),
          "the record was written BEFORE the halt, so it survives it");
    check(recorded_heap_capacity == heap_capacity_bytes && recorded_heap_peak == heap_peak_bytes,
          "the heap sample reached the record on the failing path too");
    check(strstr(log_buf, "PANIC") != NULL,
          "the panic line was printed, so a board WITH a console says why");
    check(strstr(log_buf, "boot report") != NULL,
          "the panic line points at the boot report, which is the only channel "
          "a console-less board has");
}

static void case_not_fatal(void) {
    reset();
    void* p = (void*) 1;
    if (setjmp(panic_landing) == 0) {
        p = nros_platform_alloc(TOO_BIG);
    }
    check(!saw(EV_HOOK),
          "with the knob off the fatal hook is NOT entered");
    check(p == NULL, "with the knob off the caller gets NULL, as it always did");
    check(saw(EV_LINE), "and the HEAP EXHAUSTED line is still printed");
    check(saw(EV_RECORD) && recorded_failed_size == TOO_BIG,
          "and the record still names the request, so a dump explains a board "
          "that did not halt");
}

static void case_ok(void) {
    reset();
    void* p = nros_platform_alloc(16);
    check(p != NULL, "a request the arena satisfies returns memory");
    check(!saw(EV_LINE) && !saw(EV_RECORD) && !saw(EV_HOOK),
          "and touches neither the record, the line, nor the hook");
    check(recorded_heap_capacity == heap_capacity_bytes,
          "the heap sample is taken on the SUCCEEDING path as well (W5)");
}

int main(int argc, char** argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s <fatal|not-fatal|ok>\n", argv[0]);
        return 2;
    }
    printf("case %s\n", argv[1]);
    if (strcmp(argv[1], "fatal") == 0) {
        case_fatal();
    } else if (strcmp(argv[1], "not-fatal") == 0) {
        case_not_fatal();
    } else if (strcmp(argv[1], "ok") == 0) {
        case_ok();
    } else {
        fprintf(stderr, "unknown case: %s\n", argv[1]);
        return 2;
    }
    if (failures != 0) {
        printf("%d assertion(s) failed\n", failures);
        return 1;
    }
    return 0;
}
