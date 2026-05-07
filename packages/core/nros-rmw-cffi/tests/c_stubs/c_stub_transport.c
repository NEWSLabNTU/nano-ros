/* Phase 115.G.4 — second-language smoke test for the runtime
 * transport vtable.
 *
 * This file is intentionally pure C. No Rust headers, no cbindgen
 * output, no `#include <nros/...h>`. The point of the test is to
 * prove the canonical-C-ABI is reachable by a consumer that knows
 * NOTHING about Rust beyond "it'll memcpy my struct and call my
 * fn pointers." Same shape a Zig / Python-ctypes / Lua-FFI binding
 * author would write.
 *
 * The struct shape (`nros_c_stub_transport_ops_t`) is defined
 * locally in our header; the Rust runtime's
 * `nros_rmw::NrosTransportOps` has the same `#[repr(C)]` layout.
 * If the layouts diverge, the test panics — exactly the drift signal
 * we want.
 */

#include "c_stub_transport.h"

#include <stddef.h>
#include <stdint.h>

/* ABI version number that the Rust runtime currently advertises.
 * Hard-coded here so the test can verify the *header* the consumer
 * targets is in sync with the *runtime* we link against. If the
 * Rust side bumps the version, this constant has to be updated
 * with it — the build will keep working but the runtime will
 * reject the registration with NROS_RMW_RET_INCOMPATIBLE_ABI.
 */
#define NROS_C_STUB_TRANSPORT_OPS_ABI_VERSION_V1 1u

/* Per-callback hit counters. Static-private; reach them through
 * the accessor fns below. (Raw `extern` statics get GC'd by the
 * linker when no consumer references them at link time.) */
static uint32_t s_open_calls = 0;
static uint32_t s_close_calls = 0;
static uint32_t s_write_calls = 0;
static uint32_t s_read_calls = 0;

uint32_t nros_c_stub_get_open_calls(void) { return s_open_calls; }
uint32_t nros_c_stub_get_close_calls(void) { return s_close_calls; }
uint32_t nros_c_stub_get_write_calls(void) { return s_write_calls; }
uint32_t nros_c_stub_get_read_calls(void) { return s_read_calls; }

/* Four callbacks. Plain C; no Rust involvement. */
static int32_t stub_open(void *user_data, const void *params) {
    (void)user_data;
    (void)params;
    s_open_calls++;
    return 0; /* NROS_RET_OK */
}

static void stub_close(void *user_data) {
    (void)user_data;
    s_close_calls++;
}

static int32_t stub_write(void *user_data, const uint8_t *buf, size_t len) {
    (void)user_data;
    (void)buf;
    (void)len;
    s_write_calls++;
    return 0; /* NROS_RET_OK */
}

static int32_t stub_read(void *user_data, uint8_t *buf, size_t len, uint32_t timeout_ms) {
    (void)user_data;
    (void)buf;
    (void)len;
    (void)timeout_ms;
    s_read_calls++;
    return 0; /* zero bytes received */
}

void nros_c_stub_make_ops(nros_c_stub_transport_ops_t *out) {
    if (out == NULL) {
        return;
    }
    out->abi_version = NROS_C_STUB_TRANSPORT_OPS_ABI_VERSION_V1;
    out->_reserved = 0;
    out->user_data = (void *)(uintptr_t)0xC0FFEEu;
    out->open = stub_open;
    out->close = stub_close;
    out->write = stub_write;
    out->read = stub_read;
}

void nros_c_stub_reset_counters(void) {
    s_open_calls = 0;
    s_close_calls = 0;
    s_write_calls = 0;
    s_read_calls = 0;
}
