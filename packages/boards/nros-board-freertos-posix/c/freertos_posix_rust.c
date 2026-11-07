/* phase-548 W1 — the C half of the RUST entry on the FreeRTOS POSIX board.
 *
 * The C/C++ road's `freertos_posix_entry.c` owns `main()` and does its host
 * set-up inline. A Rust image's `main` is the generated entry's, which calls
 * `<FreertosPosix as BoardEntry>::run`, so the same set-up is reached through
 * these functions instead. `freertos_posix_hooks.c` (kernel hooks, the
 * diagnostics console) is shared by both roads unchanged; this file is linked
 * by the Rust board crate only. */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

extern void nros_platform_register_log_writer(void (*writer)(uint8_t, const uint8_t*, uintptr_t,
                                                             const uint8_t*, uintptr_t),
                                              void (*flusher)(void));

/* Application output: stdout, which is what an e2e test (`nros run --expect`)
 * greps. Diagnostics go to stderr through `nros_board_freertos_console_write`
 * and the log writer below, as on the C road. */
void nros_freertos_posix_stdout_write(const char* s) {
    if (s != NULL) {
        fputs(s, stdout);
    }
}

/* Same format and stream as the C road's `board_log_writer`, so a record reads
 * the same whichever language the image is in. */
static void board_log_writer(uint8_t severity, const uint8_t* name_ptr, uintptr_t name_len,
                             const uint8_t* msg_ptr, uintptr_t msg_len) {
    const char* label;
    switch (severity) {
    case 0:
        label = "TRACE";
        break;
    case 1:
        label = "DEBUG";
        break;
    case 2:
        label = "INFO";
        break;
    case 3:
        label = "WARN";
        break;
    case 4:
        label = "ERROR";
        break;
    case 5:
        label = "FATAL";
        break;
    default:
        label = "?";
        break;
    }
    if (name_len == 0 || name_ptr == NULL) {
        fprintf(stderr, "[%s] %.*s\n", label, (int)msg_len, (const char*)msg_ptr);
    } else {
        fprintf(stderr, "[%s] %.*s: %.*s\n", label, (int)name_len, (const char*)name_ptr,
                (int)msg_len, (const char*)msg_ptr);
    }
}

static void board_log_flush(void) {
    fflush(stderr);
}

/* Before the scheduler starts: unbuffered stdio (a piped stdout is fully
 * buffered by glibc, and a test harness would never see the banner — issue
 * #194's shape on threadx-linux), then the platform log writer. Idempotent. */
void nros_freertos_posix_host_setup(void) {
    setvbuf(stdout, NULL, _IONBF, 0);
    setvbuf(stderr, NULL, _IONBF, 0);
    nros_platform_register_log_writer(board_log_writer, board_log_flush);
}

void nros_freertos_posix_exit(int code) {
    fflush(NULL);
    exit(code);
}

/* The shared FreeRTOS entry calls these trace hooks; the board has no trace
 * buffer (the mps2 board's `trace/trace_dump.c` is a Cortex-M ring), so they
 * are the same no-ops that file compiles when tracing is off. */
void nros_trace_scheduler_started(void) {}
void nros_trace_trigger_and_dump(void) {}
