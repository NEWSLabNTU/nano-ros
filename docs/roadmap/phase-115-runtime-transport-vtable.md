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

### A. Trait shape (Rust core)

`nros-rmw` (or per-RMW crate) gains a trait:

```rust
pub trait CustomTransport: Send {
    type Params: ?Sized;
    fn open(&mut self, params: &Self::Params) -> nros_ret_t;
    fn close(&mut self);
    fn write(&mut self, buf: &[u8]) -> nros_ret_t;
    fn read(&mut self, buf: &mut [u8], timeout_ms: u32) -> Result<usize, nros_ret_t>;
}
```

`zpico-platform-custom` (new feature) provides a `Platform` impl that delegates to a `dyn CustomTransport`. XRCE side provides the equivalent via `uxr_set_custom_transport_callbacks` already exposed by the C client.

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

- [ ] **115.A** Define `CustomTransport` trait in `nros-rmw` (or per-RMW crate). Document send/sync constraints.
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

- Risk: ABI commitment. `nros_transport_ops_t` field order must be stable. Lock it before 1.0.
- Risk: re-entrancy. The trait must specify whether `read` may be called from a different thread than `write`. v1: same thread only; document.
- Out of scope: dust-dds DDS support (file as 115.X follow-up).
- Out of scope: zero-copy custom transports (Phase 99 scope).
