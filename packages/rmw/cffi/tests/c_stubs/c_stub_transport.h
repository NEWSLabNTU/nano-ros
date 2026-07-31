/* Phase 115.G.4 — second-language smoke test for the runtime
 * transport vtable.
 *
 * Mimics what a non-Rust consumer (e.g. a C application, a Zig
 * binding, a Python ctypes user) would write to register a custom
 * transport via the canonical-C-ABI surface.
 *
 * The four callbacks below are ordinary C functions; the stub
 * struct is filled in via plain C aggregate-init. No Rust types
 * involved on this side of the FFI.
 */

#ifndef NROS_C_STUB_TRANSPORT_H
#define NROS_C_STUB_TRANSPORT_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Mirror of `nros_rmw::NrosTransportOps`. Single ABI — same
 * `#[repr(C)]` layout the Rust runtime exposes. */
typedef struct nros_c_stub_transport_ops {
    uint32_t abi_version;
    uint32_t _reserved;
    void *user_data;
    int32_t (*open)(void *user_data, const void *params);
    void (*close)(void *user_data);
    int32_t (*write)(void *user_data, const uint8_t *buf, size_t len);
    int32_t (*read)(void *user_data, uint8_t *buf, size_t len, uint32_t timeout_ms);
} nros_c_stub_transport_ops_t;

/* Per-callback hit counters, exposed via accessor functions (not
 * raw statics) so they survive `--gc-sections` even when no Rust
 * code references them at link time. */
uint32_t nros_c_stub_get_open_calls(void);
uint32_t nros_c_stub_get_close_calls(void);
uint32_t nros_c_stub_get_write_calls(void);
uint32_t nros_c_stub_get_read_calls(void);

/* Hand a freshly-populated `nros_c_stub_transport_ops_t` back to the
 * caller — fills in `abi_version`, the four fn pointers, and a
 * `user_data` cookie of `0xC0FFEE`. The Rust test treats the
 * returned struct as opaque (it cares only that registration
 * succeeds + abi_version is right). */
void nros_c_stub_make_ops(nros_c_stub_transport_ops_t *out);

/* Reset the four counters to 0. Test fixtures call between cases
 * so per-test counts are clean. */
void nros_c_stub_reset_counters(void);

#ifdef __cplusplus
}
#endif

#endif /* NROS_C_STUB_TRANSPORT_H */
