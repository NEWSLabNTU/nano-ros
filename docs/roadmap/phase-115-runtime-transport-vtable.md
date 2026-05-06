# Phase 115: Runtime Transport Vtable for nros-c

**Goal:** Expose a `nros_set_custom_transport(struct nros_transport_ops *ops)` C API so users can plug a custom transport (USB-CDC, BLE, RS-485, semihosting bridge) at runtime without changing board crate, Cargo features, or rebuilding.

**Status:** Core surface complete (115.A / 115.C / 115.D / 115.E / 115.G / 115.I). Backend coverage today: XRCE-DDS native; zenoh-pico (115.B) and dust-DDS (115.H) deferred to follow-up phases (115.X-zenoh, 115.X-dds). Arduino library hook (115.J) deferred to Phase 23.
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

- [x] **115.A** `nros_rmw::custom_transport` — `NrosTransportOps` (`#[repr(C)]`, four `unsafe extern "C" fn` fields + `user_data: *mut c_void`, `unsafe impl Send + Sync` on the vtable struct itself). Storage is `static SLOT: Mutex<Option<NrosTransportOps>>` (the existing `nros_rmw::sync::Mutex`, no extra deps). Public API: `set_custom_transport(Option<NrosTransportOps>)` (unsafe — caller owns the threading contract), `peek_custom_transport()`, `take_custom_transport()`. Module-level docs cover the threading contract (no concurrent read/write, no ISR invocation, `user_data` outlives `close`) and the no-`dyn` rationale (cross-link to § A.1). 3 unit tests: lifecycle (set → peek → take → empty), explicit clear, and `Copy + Send + Sync` static assertion. (`<this commit>`)
- [ ] **115.B — DEFERRED to 115.X-zenoh.** `zpico-platform-custom` crate. zenoh-pico's custom-link API (`_z_link_t` extension) needs a per-platform C-side shim plus a `link-custom` feature in `zpico-sys`. Significant plumbing; tracked separately. Zenoh users with custom transports today fork a `zpico-platform-*` crate; the runtime hook on XRCE (115.E) covers the primary use case.
- [x] **115.C** `nros-c` C API: `nros_transport_ops_t` (`#[repr(C)]` — same layout as `nros_rmw::NrosTransportOps`), `nros_set_custom_transport(*const ops)`, `nros_clear_custom_transport()`, `nros_has_custom_transport()`. cbindgen-emitted into `nros_generated.h`. Docs cover threading + return-code conventions. (`<this commit>`)
- [x] **115.D** `nros-cpp` C++ wrapper: `nros::TransportOps` POD-style struct (no STL), `nros::set_custom_transport(const TransportOps&) -> Result`, `nros::clear_custom_transport()`, `nros::has_custom_transport() -> bool`. Inline header `<nros/transport.hpp>` + Rust-side FFI in `nros-cpp/src/transport.rs`. (`<this commit>`)
- [x] **115.E** XRCE plumbing — `nros_rmw_xrce::init_transport_from_custom_ops(framing)` drains `nros_rmw::take_custom_transport()`, copies the four fn pointers + user_data into XRCE-local trampoline state, and registers C trampolines with `uxr_set_custom_transport_callbacks` + `uxr_init_custom_transport`. Bridges the v1 ABI mismatch: XRCE's `open` / `close` callbacks take `*mut uxrCustomTransport`; ours take `*mut c_void user_data`. The trampolines pull `user_data` from the static slot and forward. (`<this commit>`)
- [ ] **115.F — DEFERRED.** Loopback example `examples/qemu-arm-baremetal/c/zenoh/custom-transport-loopback/`. Depends on 115.B (zenoh path). XRCE-side loopback would need MicroXRCEAgent in the test harness; defer until either the agent fixture lands or 115.B unblocks.
- [x] **115.G** Integration test at `packages/xrce/nros-rmw-xrce/tests/custom_transport.rs`. 2 tests passing: `set_custom_transport_round_trips_through_xrce_bridge` (register → peek → take via XRCE bridge → confirm slot drained → second take returns false) + `clear_via_set_none` (explicit clear via `set_custom_transport(None)`). Stub callbacks count invocations; the test does NOT open an XRCE session (would need MicroXRCEAgent). The 3-test slot-lifecycle suite in `nros-rmw/src/custom_transport.rs` covers the storage layer; this test covers the XRCE bridge. (`<this commit>`)
- [ ] **115.H — DEFERRED to 115.X-dds.** dust-DDS plug-in path. dust-dds requires implementing a custom `RtpsUdpTransportParticipantFactory`-equivalent. Larger surface; design doc to land in a separate phase.
- [x] **115.I** `book/src/porting/custom-transport.md` (new). Covers when to use, Rust / C / C++ examples, threading contract (no concurrent read/write, no ISR invocation, `user_data` lifetime), return-code conventions, framing semantics per backend, and a per-backend coverage table. Linked from `book/src/SUMMARY.md` under Porting. mdbook builds clean. (`<this commit>`)
- [ ] **115.J — DEFERRED to Phase 23.** Arduino library reuse. Phase 23 (Arduino precompiled lib) hasn't started; once it does, `nros::set_serial_transport(&Serial)` / `nros::set_wifi_udp_transport(...)` should reuse this hook instead of inventing a parallel API.

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
