# Phase 115: Runtime Transport Vtable for nros-c

**Goal:** Expose a `nros_set_custom_transport(struct nros_transport_ops *ops)` C API so users can plug a custom transport (USB-CDC, BLE, RS-485, semihosting bridge) at runtime without changing board crate, Cargo features, or rebuilding.

**Status:** Not Started
**Priority:** Medium
**Depends on:** Phase 79 (unified platform abstraction), Phase 102 (RMW API alignment)
**Related:** `docs/research/sdk-ux/SYNTHESIS.md` UX-22; reference `rmw_uros_set_custom_transport` in micro-ROS

---

## Overview

Today, swapping serial-USB ↔ UDP requires editing the board crate, Cargo features (`ethernet` vs `wifi` vs `serial`), and `config.toml`. Users with custom hardware bridges (USB-CDC, BLE GATT, RS-485 with framing) have no extension hook — they fork a board crate.

micro-ROS solves this with `rmw_uros_set_custom_transport(framing, params, open, close, write, read)`. Four function pointers, runtime-settable. The same RMW core uses them.

This phase brings an equivalent C-side hook to nano-ros. It is intentionally orthogonal to Phase 22 (board transport features); the static path stays for users who want compile-time elimination of unused transports. The runtime path is opt-in.

---

## Architecture

### A. Vtable shape (Rust core)

`nros-rmw` exposes a fn-pointer vtable that mirrors the C ABI 1:1.
**No trait, no `dyn`, no `Box`** — `dyn` would force `alloc`, which
the project deliberately avoids on its no_std backends (same
constraint that landed in Phase 110 review and the existing XRCE
`init_transport` callback shape).

```rust
/// Phase 115 — runtime-pluggable custom transport. Caller fills in
/// the four fn pointers, hands the struct to `set_custom_transport`,
/// and the active backend treats it as the read/write surface for
/// every wire frame.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct NrosTransportOps {
    /// Opaque caller context, threaded back into every callback.
    pub user_data: *mut core::ffi::c_void,
    /// Open the underlying medium (e.g. open the UART, claim the
    /// USB-CDC endpoint). `params` is opaque per-transport metadata.
    pub open: unsafe extern "C" fn(user_data: *mut c_void, params: *const c_void) -> nros_ret_t,
    /// Tear the transport down; complement of `open`.
    pub close: unsafe extern "C" fn(user_data: *mut c_void),
    /// Send `len` bytes; returns `NROS_RET_OK` on success.
    pub write: unsafe extern "C" fn(user_data: *mut c_void, buf: *const u8, len: usize) -> nros_ret_t,
    /// Receive up to `len` bytes within `timeout_ms`. Returns the
    /// non-negative byte count on success, a negative `nros_ret_t`
    /// on error / timeout.
    pub read: unsafe extern "C" fn(user_data: *mut c_void, buf: *mut u8, len: usize, timeout_ms: u32) -> i32,
}

// SAFETY: the struct is just four fn pointers + a *mut. The caller
// owns synchronisation of `user_data` per the threading contract
// documented in book/src/porting/custom-transport.md.
unsafe impl Send for NrosTransportOps {}
unsafe impl Sync for NrosTransportOps {}
```

Storage is a single `static AtomicCell<Option<NrosTransportOps>>`
(or a `static mut` guarded by `ffi_guard` on backends without
atomic-cell) — registered once at boot, read on every transport
hit. No allocation, no per-call indirection cost beyond the fn-ptr
load.

`zpico-platform-custom` (new feature) provides a `Platform` impl
whose `tcp_*` / `udp_*` / `serial_*` shims call straight through to
the registered `NrosTransportOps`. XRCE side passes the four fn
pointers verbatim to `uxr_set_custom_transport_callbacks` — the
existing `nros-rmw-xrce::init_transport` already takes the same
shape.

### A.1 Why fn-ptr vtable, not a Rust trait

Three reasons, in order of importance:

1. **alloc-free.** A `Box<dyn CustomTransport>` lands the alloc
   crate on every no_std backend that wants to use the runtime hook.
   nano-ros's bare-metal / FreeRTOS / NuttX / ThreadX targets ship
   without a global allocator on the default feature flags, so
   `dyn` is a non-starter. fn pointers cost zero static memory.
2. **C ABI parity.** The user-facing surface is `nros_transport_ops_t`
   (a struct of fn pointers and a `void*`). A Rust-side fn-ptr
   vtable means the `set_custom_transport` C entry just memcpys the
   incoming struct into the static — no glue, no shims, no
   trampolines.
3. **Matches XRCE's existing shape.** `uxr_set_custom_transport_callbacks`
   already takes 4 raw fn pointers; the Rust wrapper at
   `nros-rmw-xrce::init_transport` likewise. A trait would just be
   an extra layer that has to be type-erased into fn pointers anyway.

### B. C API

```c
typedef struct nros_transport_ops {
    void *user_data;
    nros_ret_t (*open)(void *user_data, const void *params);
    void       (*close)(void *user_data);
    nros_ret_t (*write)(void *user_data, const uint8_t *buf, size_t len);
    int32_t    (*read)(void *user_data, uint8_t *buf, size_t len, uint32_t timeout_ms);
} nros_transport_ops_t;

nros_ret_t nros_set_custom_transport(const nros_transport_ops_t *ops, const void *params);
```

Called *before* `nros_support_init`. Stored in a static. The platform crate calls into the registered ops via the trait.

C++ side: `nros::set_custom_transport(nros::TransportOps&& ops)` with the same shape.

### C. RMW coverage

- **Zenoh** — Zenoh-pico already supports custom links via `_z_link_t` callbacks; wire the trait through.
- **XRCE** — exposes `uxr_set_custom_transport_callbacks` natively; pass-through.
- **DDS** — dust-dds requires a custom transport plug-in; v1 of this phase punts on DDS (file as 115.X follow-up).

### D. Examples

- `examples/qemu-arm-baremetal/c/zenoh/custom-transport-loopback/` — minimal demo using a ring-buffer "transport" between two threads.
- `examples/freertos-usb-cdc-bridge/` — real-world USB-CDC bridge example for FreeRTOS QEMU (TinyUSB-style stub).

### E. Interaction with the static path

The existing `ethernet` / `wifi` / `serial` features stay. `zpico-platform-custom` is a new mutual-exclusive sibling. Compile-time check enforces only one is selected. Runtime registration is required when `platform-custom` is enabled, optional otherwise (no-op).

---

## Work Items

- [ ] **115.A** Define `NrosTransportOps` fn-ptr vtable in `nros-rmw`. `#[repr(C)]`, four `unsafe extern "C" fn` fields plus `user_data: *mut c_void`. Add `set_custom_transport(&NrosTransportOps)` Rust API + matching `static AtomicCell<Option<NrosTransportOps>>` storage. **No trait, no `dyn`, no `Box`** — see § A.1 for rationale. Document send/sync contract on `user_data`.
- [ ] **115.B** `zpico-platform-custom` crate — new mutual-exclusive transport variant.
- [ ] **115.C** `nros-c` C API: `nros_transport_ops_t`, `nros_set_custom_transport`. cbindgen-emitted header.
- [ ] **115.D** `nros-cpp` C++ wrapper.
- [ ] **115.E** XRCE plumbing — wire `nros_set_custom_transport` to `uxr_set_custom_transport_callbacks`.
- [ ] **115.F** Loopback example — `examples/qemu-arm-baremetal/c/zenoh/custom-transport-loopback/`.
- [ ] **115.G** Integration test in `nros_tests/` (POSIX, single process, ring-buffer transport).
- [ ] **115.H** DDS path — file as `115.X follow-up` or design doc.
- [ ] **115.I** `book/src/porting/custom-transport.md` (new). Documents callback contract, threading, framing requirements.
- [ ] **115.J** Phase 23 Arduino library uses this hook for `set_microros_transports`-equivalent (`nros::set_serial_transport(&Serial)`, `nros::set_wifi_udp_transport(...)`).

**Files:**
- `packages/zpico/zpico-platform-custom/` (new crate)
- `packages/core/nros-c/include/nros/transport.h` (new)
- `packages/core/nros-cpp/include/nros/transport.hpp` (new)
- `packages/xrce/.../custom_transport.rs` (wiring)
- `examples/qemu-arm-baremetal/c/zenoh/custom-transport-loopback/` (new)
- `book/src/porting/custom-transport.md` (new)
- `nros_tests/tests/custom_transport.rs` (new)

---

## Acceptance criteria

- C user registers 4 callbacks via `nros_set_custom_transport`, then `nros_support_init` succeeds and pub/sub work end-to-end on the loopback example.
- The same callbacks compile and link on FreeRTOS, NuttX, ThreadX, Zephyr, bare-metal targets.
- Compile-time check rejects co-enabling `platform-custom` with another `platform-*`.
- Phase 23 Arduino library reuses the hook (no duplicate transport API).
- `book/src/porting/custom-transport.md` documents threading model, ISR safety (callbacks must NOT be called from ISR), framing requirements.

## Notes

- Risk: ABI commitment. `nros_transport_ops_t` field order must be stable. Lock it before 1.0. The Rust-side `NrosTransportOps` is `#[repr(C)]` so the two share a single layout — no parallel definitions to drift.
- Risk: re-entrancy. The vtable contract must specify whether `read` may be called from a different thread than `write`. v1: same thread only; document.
- Risk: registration after `nros_support_init`. v1 rejects late registration with `NROS_RET_ALREADY_INIT`. Documented in `book/src/porting/custom-transport.md`.
- Out of scope: dust-dds DDS support (file as 115.X follow-up).
- Out of scope: zero-copy custom transports (Phase 99 scope).
