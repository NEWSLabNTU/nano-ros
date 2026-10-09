/*
 * board_threadx_linux.c — board-specific glue for nros ThreadX Linux sim.
 *
 * The shared `tx_application_define` + byte-pool / app-thread plumbing
 * lives in `nros_board_common`'s `threadx_hooks.c`. This file fills in
 * the three weak hooks that file calls into, plus the
 * `nros_threadx_set_config` FFI setter whose signature differs from
 * the RISC-V sibling overlay (Linux carries `interface_name`).
 *
 * Networking goes through nsos-netx (NetX BSD shim over host POSIX
 * sockets) — no NetX Duo TCP/IP stack, no IP instance, no packet pool,
 * no ARP, no veth/TAP driver. Application's `nx_bsd_*` calls are
 * forwarded directly to the host kernel.
 */

#include <errno.h>
#include <pthread.h>
#include <signal.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>

#include "tx_api.h"

/* ---- Configuration (set from Rust before tx_kernel_enter) ---- *
 * IP/MAC/interface fields are accepted but mostly ignored — NSOS
 * uses the host kernel's networking, so no per-instance IP setup is
 * needed. We still cache IP/MAC for the RNG seed derivation. */
static uint8_t cfg_ip[4]  = {127, 0, 0, 1};
static uint8_t cfg_mac[6] = {0x02, 0x00, 0x00, 0x00, 0x00, 0x00};

/* FFI: called from Rust to set config. Signature kept for
 * compatibility with the Rust glue. The netmask / gateway /
 * interface_name parameters are ignored under NSOS.
 *
 * Return type is `void` by contract — Phase 214.A.1: this impl
 * is pure memcpy into static storage (NULL-guarded), has no
 * meaningful failure modes, and is called from board startup
 * before networking begins. Callers (e.g. `startup.c`) do not
 * capture a return code. If a future revision adds I/O or
 * validation that can fail, this contract changes; bump to
 * `int` + propagate. */
void nros_threadx_set_config(
    const uint8_t *ip,
    const uint8_t *netmask,
    const uint8_t *gateway,
    const uint8_t *mac,
    const char *interface_name)
{
    (void)netmask;
    (void)gateway;
    (void)interface_name;
    if (ip  != NULL) { memcpy(cfg_ip,  ip,  4); }
    if (mac != NULL) { memcpy(cfg_mac, mac, 6); }
}

/* ---- Weak-hook impls (overrides the defaults in threadx_hooks.c) ---- */

void nros_board_log(const char *s)
{
    if (s) { fputs(s, stdout); }
}

int nros_board_init_eth(void)
{
    /* NSOS uses the host kernel's BSD sockets — nothing to do at
     * the NetX layer. */
    return 0;
}

void nros_board_compute_rng_seed(uint32_t *out)
{
    if (!out) { return; }
    uint32_t seed = ((uint32_t)cfg_ip[0] << 24) | ((uint32_t)cfg_ip[1] << 16)
                  | ((uint32_t)cfg_ip[2] << 8)  | (uint32_t)cfg_ip[3];
    seed = seed * 2654435761u;  /* Knuth multiplicative hash */
    seed ^= ((uint32_t)cfg_mac[4] << 8) | (uint32_t)cfg_mac[5];
    *out = seed;
}

/* ---- Log line -> host stderr (issue 0585) ----
 *
 * Two constraints meet here, and each rules out the obvious answer to the
 * other.
 *
 * We cannot call `write()`: the ThreadX Linux port defines a WEAK `write`
 * that does not reach host file descriptors, so a normal call is captured by
 * it and the bytes vanish. The raw syscall is what bypasses that.
 *
 * And the syscall NUMBER cannot be written down. It is per-ARCHITECTURE, not
 * per-OS — `write` is 1 on x86_64 and 64 on every asm-generic port (aarch64,
 * riscv64, loongarch64), where 1 is `io_destroy`. The Rust side hardcoded the
 * x86 value, so off x86 it issued an unrelated syscall that failed, the
 * return was discarded, and the image ran silently mute: booted, entered
 * Rust, flushed, exited 0, printed nothing.
 *
 * Doing it here rather than in Rust is the point. `<sys/syscall.h>` supplies
 * `SYS_write` for whatever host is compiling — the same headers the `libc`
 * crate's constants are generated from — so the number cannot be wrong for a
 * host we did not anticipate. The Rust side previously carried a hand-written
 * per-arch table; a table is a thing to keep correct, and this is not.
 *
 * `len` is a byte count, not a C string: the caller's buffer is not NUL
 * terminated. A short write is not retried — this is a log line, and the one
 * failure mode worth ruling out was silence, not truncation.
 */
void nros_board_log_write_stderr(const uint8_t *buf, size_t len)
{
    if (buf == NULL || len == 0) { return; }
    (void)syscall(SYS_write, STDERR_FILENO, buf, len);
}

static void board_stderr_line(const char *s)
{
    nros_board_log_write_stderr((const uint8_t *)s, strlen(s));
}

/* ---- A termination signal ENDS the image (issue 1741) ----
 *
 * A threadx-linux image is a host process, and `timeout`, a test harness and
 * a shell's Ctrl-C all stop one with ONE `SIGTERM`/`SIGINT`. Measured
 * (2026-10-08, the in-tree C service-server over Cyclone): the image caught
 * the `SIGTERM` (`SigCgt` bit 15) and was still alive 15 s later — and one
 * such image, started as `timeout 6 …`, lived 34 days.
 *
 * Two facts made that happen, and each is enough on its own:
 *
 *   1. The C/C++ examples install `signal(SIGTERM, handler)` — the idiomatic
 *      graceful shutdown: set a flag, `nros_executor_cancel()`, let the spin
 *      return and tear down. That shutdown is RUN BY THE RTOS: the app thread
 *      has to be scheduled to see the cancel. When the ThreadX scheduler is
 *      wedged (the measured image: every pthread parked on `_tx_linux_mutex`
 *      before the signal ever arrived), the handler sets its flag, nothing
 *      ever reads it, and the process survives every `SIGTERM` it is sent.
 *   2. Even a shutdown that completes did not end the process: the app thread
 *      returned into `threadx_hooks.c`, which (correctly, for a real MCU) just
 *      lets the thread finish — and the scheduler idles forever.
 *
 * So the guard below, installed before `tx_kernel_enter()`:
 *
 *   * BLOCKS `SIGTERM`/`SIGINT` in the boot thread, so every thread the
 *     kernel, the port and the RMW create later inherits the mask (the port's
 *     timer thread is created inside `tx_kernel_enter`, after this). No
 *     ThreadX thread is interrupted by one any more — the port suspends and
 *     resumes its threads with SIGUSR1/SIGUSR2 and is not written to survive
 *     an application handler landing mid-switch;
 *   * receives them on ONE host thread with `sigwaitinfo`, and there:
 *       - SIG_IGN   -> ignored, as asked;
 *       - SIG_DFL   -> the default action: the process dies by the signal
 *                      (what every Rust threadx-linux image relied on before);
 *       - a handler -> it is CALLED (from this thread, not a signal frame),
 *                      and the image then has NROS_THREADX_LINUX_TERM_GRACE_MS
 *                      to finish by itself. A second signal, or the grace
 *                      running out, ends it by the default action, with a
 *                      line on stderr saying which.
 *
 * And `nros_board_app_returned()` below makes a returned C/C++ app END the
 * process — the statement `nros-board-freertos-posix`'s entry makes for the
 * same reason: the image is a host process, so ending is `exit`.
 *
 * The grace is deliberately below the harness's own kill-after
 * (`nros_tests::process::KILL_GRACE`, `NROS_KILL_GRACE_S`): the image ends
 * by its own hand before any escalation has to fire.
 */
#define NROS_THREADX_LINUX_TERM_GRACE_MS 2000

static sigset_t term_signals;
static int term_guard_installed = 0;

static void term_by_default_action(int sig)
{
    sigset_t one;
    fflush(NULL);
    signal(sig, SIG_DFL);
    sigemptyset(&one);
    sigaddset(&one, sig);
    (void)pthread_sigmask(SIG_UNBLOCK, &one, NULL);
    (void)raise(sig);
    /* Not reached: the default action of SIGTERM/SIGINT terminates. */
    _exit(128 + sig);
}

static void *term_guard_thread(void *arg)
{
    (void)arg;
    for (;;) {
        siginfo_t info;
        struct sigaction cur;
        int sig = sigwaitinfo(&term_signals, &info);
        if (sig < 0) {
            if (errno == EINTR) { continue; }
            board_stderr_line("nros: termination guard: sigwaitinfo failed - terminating\n");
            term_by_default_action(SIGTERM);
        }
        if (sigaction(sig, NULL, &cur) != 0) { term_by_default_action(sig); }
        if (!(cur.sa_flags & SA_SIGINFO)) {
            if (cur.sa_handler == SIG_IGN) { continue; }
            if (cur.sa_handler == SIG_DFL) { term_by_default_action(sig); }
        }

        /* The application asked to shut down gracefully: let it try. */
        if (cur.sa_flags & SA_SIGINFO) {
            cur.sa_sigaction(sig, &info, NULL);
        } else {
            cur.sa_handler(sig);
        }

        struct timespec grace = {
            .tv_sec = NROS_THREADX_LINUX_TERM_GRACE_MS / 1000,
            .tv_nsec = (long)(NROS_THREADX_LINUX_TERM_GRACE_MS % 1000) * 1000000L,
        };
        int again;
        do {
            again = sigtimedwait(&term_signals, &info, &grace);
        } while (again < 0 && errno == EINTR);
        if (again > 0) {
            board_stderr_line("nros: second termination signal - terminating without "
                              "waiting for the application\n");
        } else {
            board_stderr_line("nros: the application did not finish within its "
                              "termination grace - terminating\n");
        }
        term_by_default_action(sig);
    }
    return NULL;
}

/* Install once, before `tx_kernel_enter()`. Every threadx-linux entry reaches
 * it: the C/C++ `startup.c::main` and the Rust board's pre-kernel step. */
void nros_threadx_linux_install_termination_guard(void)
{
    pthread_t tid;
    if (term_guard_installed) { return; }
    term_guard_installed = 1;
    sigemptyset(&term_signals);
    sigaddset(&term_signals, SIGTERM);
    sigaddset(&term_signals, SIGINT);
    if (pthread_sigmask(SIG_BLOCK, &term_signals, NULL) != 0
        || pthread_create(&tid, NULL, term_guard_thread, NULL) != 0) {
        /* Leave the signals deliverable rather than swallowed: with no guard
         * thread, the old behaviour is the only safe one. */
        (void)pthread_sigmask(SIG_UNBLOCK, &term_signals, NULL);
        board_stderr_line("nros: termination guard NOT installed - a termination "
                          "signal may not end this image\n");
        return;
    }
    (void)pthread_detach(tid);
}

/* Overrides the weak no-op in `threadx_hooks.c`: the C/C++ app thread
 * returned. `_exit`, not `exit`: the RMW's threads are still running, and an
 * atexit handler or destructor re-entering them (or the ThreadX port) from a
 * process already tearing down is not a shutdown anyone has tested. Every
 * stream is flushed first, so nothing the app printed is lost. The status is
 * the app's own (issue 1752): `<nros/app_main.h>`'s VOID shim hands
 * `nros_app_main`'s return value, already an exit code, to `threadx_hooks.c`,
 * which passes it here — so a failed app no longer reads as success. */
void nros_board_app_returned(int exit_code)
{
    fflush(NULL);
    _exit(exit_code);
}
