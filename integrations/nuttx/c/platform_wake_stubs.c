/* Phase 157.C.15 — `nros_platform_wake_*` stubs for the NuttX make
 * build.
 *
 * The cmake QEMU bring-up (Phase 144.6) for NuttX skips these because
 * its example binaries don't link `nros-node`'s `NodeWake` path — the
 * Rust shim wraps `app_main()` directly without instantiating an
 * Executor with `node_wake`. The canonical NuttX make-build (Phase
 * 157.C) DOES exercise `NodeWake::new()` via the executor's spin
 * loop. Without these symbols the kernel link fails with `undefined
 * reference to nros_platform_wake_*`.
 *
 * Returning 0 from `storage_size()` makes `NodeWake::new()` return
 * `None` per the contract in
 * `packages/core/nros-node/src/executor/node_wake.rs`. The executor
 * then falls back to driving the transport for the full timeout
 * instead of using a kernel-native binary semaphore — works
 * correctly, just slightly higher P99 wake latency under contention.
 *
 * A proper NuttX-native wake implementation would back these stubs
 * with `sem_t` (POSIX semaphore — NuttX provides them) following the
 * pattern in `packages/core/nros-platform-{posix,freertos,threadx}/
 * src/platform.c`. Tracked as a 158.x follow-up. */

#include <stddef.h>
#include <stdint.h>

size_t nros_platform_wake_storage_size(void) {
    return 0;
}

size_t nros_platform_wake_storage_align(void) {
    return 1;
}

int8_t nros_platform_wake_init(void *w) {
    (void)w;
    return -1;
}

int8_t nros_platform_wake_drop(void *w) {
    (void)w;
    return 0;
}

int8_t nros_platform_wake_wait_ms(void *w, uint32_t timeout_ms) {
    (void)w;
    (void)timeout_ms;
    return -1;
}

int8_t nros_platform_wake_signal(void *w) {
    (void)w;
    return -1;
}

int8_t nros_platform_wake_signal_from_isr(void *w) {
    (void)w;
    return -1;
}
