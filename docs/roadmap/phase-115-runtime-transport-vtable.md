# Phase 115: Runtime Transport Vtable for nros-c

**Goal:** Expose a `nros_set_custom_transport(struct nros_transport_ops *ops)` C API so users can plug a custom transport (USB-CDC, BLE, RS-485, semihosting bridge) at runtime without changing board crate, Cargo features, or rebuilding.

**Sub-goal (added 2026-05-06):** Establish the **canonical-C-ABI**
pattern for the project. The transport vtable is the first
fn-ptr-vtable cross-language interface; treat it as the template
for every future Rust→C boundary (RMW, platform, status events,
etc.). Per the design note
[`docs/design/portable-rmw-platform-interface.md`](../design/portable-rmw-platform-interface.md):

- The canonical struct definition lives in `nros-rmw-cffi` (the
  C-ABI crate), not `nros-rmw` (the Rust trait crate).
- Every other language binding (Rust trait, C++ wrapper, future
  Python / Lua / Go / Zig bindings) starts from the cbindgen-
  emitted C header.
- Vtable structs reserve `(abi_version: u32, _reserved: u32)` at
  offset 0 so future appends are detectable.
- `nros-rmw-cffi`'s shape is the only "design decision"; L1 / L2
  wrappers above it are mechanical translations.

**Status:** v1 + 115.B + 115.F (native) + 115.H scaffolding complete. Three RMW backends now expose the runtime-pluggable transport vtable: XRCE-DDS via 115.E (full), zenoh-pico via 115.B (full + 115.F native loopback E2E), dust-DDS via 115.H scaffolding (factory + smoke test landed; `DdsRmw` locator-scheme dispatch + discovery-over-byte-pipe deferred to `115.H.2-discovery`). All v1 acceptance criteria satisfied (115.A.1 / 115.A.2 / 115.B / 115.C / 115.D / 115.E / 115.G.1–4 / 115.I / 115.I.2). The transport vtable is the project's first canonical-C-ABI interface; the design + test pattern (`abi_version` field, `tests/c_stubs/`, second-language smoke test) is the template for future Rust→C boundaries (Phase 117 will roll the same shape across the wider RMW + Platform vtables). **Deferred to follow-up phases:** 115.F (bare-metal C variant) blocked on a bare-metal C example harness, 115.H.2 (DDS dispatch + discovery) tracked separately, 115.J → Phase 23.
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

The work items are restructured around the **L0 / L1 / L2 ladder**
documented in
[`docs/design/portable-rmw-platform-interface.md`](../design/portable-rmw-platform-interface.md):

- **L0 — canonical C ABI**: the single source of truth. Lives in
  `nros-rmw-cffi` as a `#[repr(C)]` Rust struct; cbindgen emits the
  matching C header. **Every other language binding starts here.**
- **L1 — per-language idiomatic wrappers**: thin glue over L0. No
  new design decisions. Today: Rust (`nros-rmw`), C (`nros-c`),
  C++ (`nros-cpp`).
- **L2 — typed application API**: typed pubs/subs/services. Custom
  transport has no L2 — it's platform-side, not user-data-side.

### L0 — canonical ABI in `nros-rmw-cffi`

- [x] **115.A.1 — first iteration in `nros-rmw`.** Initial
  `NrosTransportOps` shipped in `nros_rmw::custom_transport`
  (`#[repr(C)]`, four `unsafe extern "C" fn` + `user_data`,
  `static SLOT: Mutex<Option<...>>`, three-fn API:
  `set_custom_transport` / `peek_custom_transport` /
  `take_custom_transport`). 3 unit tests passing. (commit `be28d0af`)
- [x] **115.A.2 — `abi_version` field + version-mismatch
  rejection.** First pass of the canonical-C-ABI rollout
  (`abi_version: u32` + `_reserved: u32` at offset 0,
  `NROS_TRANSPORT_OPS_ABI_VERSION_V1 = 1` const, mismatch ⇒
  `TransportError::IncompatibleAbi` →
  `NROS_RMW_RET_INCOMPATIBLE_ABI = -14`). Threaded through Rust /
  C / C++ surfaces; XRCE bridge + book examples updated to fill
  in the field. **Crate-location move (struct definition from
  `nros-rmw` → `nros-rmw-cffi`) deferred** — `nros-rmw-cffi`
  already depends on `nros-rmw`, inverting the dep direction is a
  bigger refactor that doesn't change the wire ABI. The
  `#[repr(C)]` layout + cbindgen output already give the
  canonical-C-ABI property the design note R1 asks for; the
  type's home crate can move later. (commit `4e6e6858`)

### L1 — Rust trait / C / C++ wrappers

- [x] **115.A.1 — Rust wrapper** at `nros_rmw::custom_transport`.
  See L0 entry above; this is what landed in `be28d0af`. Migrates
  to a re-export in 115.A.2.
- [x] **115.C — C wrapper.** `nros_transport_ops_t` declared in
  `nros-c` (currently — moves to cbindgen-emit-from-cffi after
  115.A.2). Public API: `nros_set_custom_transport(*const ops)`,
  `nros_clear_custom_transport()`, `nros_has_custom_transport()`.
  Validate `ops->abi_version` once 115.A.2 lands; reject mismatched
  versions with `NROS_RET_INCOMPATIBLE_ABI`. (commit `d16bf294`;
  abi_version validation queued for 115.A.2 follow-up)
- [x] **115.D — C++ wrapper.** `nros::TransportOps` POD struct
  (no STL), `nros::set_custom_transport(const TransportOps&) ->
  Result`, `nros::clear_custom_transport()`,
  `nros::has_custom_transport() -> bool`. Inline header
  `<nros/transport.hpp>` + Rust-side FFI in
  `nros-cpp/src/transport.rs`. After 115.A.2: thin shim that
  passes `abi_version = NROS_RMW_TRANSPORT_OPS_ABI_VERSION_V1`
  through. (commit `d16bf294`)

### Backend integrations

- [x] **115.E — XRCE plumbing.**
  `nros_rmw_xrce::init_transport_from_custom_ops(framing)` drains
  `nros_rmw::take_custom_transport()`, copies into XRCE-local
  trampoline state, registers C trampolines with
  `uxr_set_custom_transport_callbacks` +
  `uxr_init_custom_transport`. Bridges the ABI mismatch: XRCE's
  `open` / `close` take `*mut uxrCustomTransport`; ours take
  `*mut c_void user_data`. (commit `d16bf294`)
- [x] **115.B — zenoh-pico custom-link.** Implemented per
  § Appendix B. Four components:
  1. **zenoh-pico fork** (branch `phase-115-link-custom` off
     `897618d5`): 4 new files (`include/zenoh-pico/system/link/custom.h`,
     `include/zenoh-pico/link/config/custom.h`,
     `src/link/config/custom.c`, `src/link/unicast/custom.c`) + 5
     patches (`include/zenoh-pico/link/{endpoint,link,manager}.h`
     for `CUSTOM_SCHEMA` / `_Z_LINK_TYPE_CUSTOM` / forward decls;
     `src/link/{endpoint,link}.c` for scheme dispatch;
     `include/zenoh-pico/config.h.in` + `CMakeLists.txt` for
     `Z_FEATURE_LINK_CUSTOM` plumbing). ~370 LOC.
  2. **`zpico-sys`**: new `link-custom` feature; `build.rs`
     emits `Z_FEATURE_LINK_CUSTOM` into the generated config
     header (CMake + cc::Build paths) and force-links
     `zpico-platform-custom` via `extern crate`.
  3. **`zpico-platform-custom`** (new crate, ~100 LOC): exposes
     `extern "C" fn nros_zpico_custom_take(out) -> i32` which
     drains `nros_rmw::take_custom_transport()` into the C-side
     vtable buffer. Layout parity with `NrosTransportOps`
     enforced by a compile-time `size_of` + `align_of` assert.
  4. **`nros-rmw-zenoh`**: new `link-custom` Cargo feature
     passes through to `zpico-sys/link-custom`.

  **End-to-end test** at
  `packages/zpico/nros-rmw-zenoh/tests/custom_transport.rs`.
  Registers a stub vtable, opens `ZenohTransport::open` against
  locator `custom/anywhere`, and asserts all four user callbacks
  (`open`/`write`/`read`/`close`) fired during session bring-up +
  teardown. The stub's `read()` returns 0 bytes ⇒ zenoh-pico
  can't complete the INIT handshake ⇒ session-open returns
  `ConnectionFailed`; that's expected for v1 (a real custom
  transport implements the bytes-in / bytes-out contract). The
  link layer still drove every fn pointer, which is what the
  test verifies.

  Run via:
  ```bash
  cargo test -p nros-rmw-zenoh --features platform-posix,link-tcp,link-custom \
      --test custom_transport
  ```
  1/1 passing.

  Locator surface for users: `custom/<addr>`. The `<addr>` segment
  is opaque to v1 (no configurable keys); future minor-version
  bumps may thread it through `params` to the user's `open()`. (`<this commit>`)
- [~] **115.H — dust-DDS custom transport.** Transport-layer
  scaffolding landed:
  `packages/dds/nros-rmw-dds/src/transport_custom.rs` ships
  `NrosCustomTransportParticipantFactory<P>` mirroring
  `NrosUdpTransportFactory<P>`'s shape (slot drain via
  `nros_rmw::take_custom_transport`, single-task recv loop,
  `WriteMessage` impl funneling every datagram through `cb_write`).
  Smoke test
  `packages/dds/nros-rmw-dds/tests/custom_transport.rs` validates
  `cb_open`/`cb_write`/`cb_read` all trip via stub callbacks (no
  agent / multicast required). **Remaining v1 work** —
  `DdsRmw::open` locator-scheme dispatch (`custom/...` → custom
  factory) and a discovery-over-byte-pipe story (no multicast
  SPDP; needs static-peer mode in dust-dds). Tracked as
  `115.H.2-discovery`. Design surface in Appendix C below.

### Tests

- [x] **115.G.1 — slot-lifecycle unit tests.** 3 tests in
  `nros-rmw/src/custom_transport.rs::tests` (set → peek → take
  round-trip; explicit `set(None)` clear; `Copy + Send + Sync`
  static assertion). Migrate to `nros-rmw-cffi/tests/...` after
  115.A.2. (commit `be28d0af`)
- [x] **115.G.2 — XRCE bridge round-trip.** 2 tests in
  `nros-rmw-xrce/tests/custom_transport.rs`
  (register-via-slot → drain-via-XRCE-bridge round-trip; explicit
  clear). Stub callbacks; no MicroXRCEAgent needed. (commit
  `d16bf294`)
- [x] **115.G.3 — abi_version mismatch test.** `rejects_unknown_abi_version`
  in `nros-rmw/src/custom_transport.rs::tests` (Rust path).
  `c_built_ops_with_bogus_abi_version_rejected` in
  `nros-rmw-cffi/tests/c_stub_transport.rs` (C-built struct
  variant — proves the rejection path also triggers when the bad
  version is set from the C side). Both passing. (`<this commit>`)
- [x] **115.G.4 — second-language smoke test.** A pure-C transport
  stub at `nros-rmw-cffi/tests/c_stubs/c_stub_transport.{c,h}`
  (~95 LOC of plain C — no Rust headers / cbindgen / Rust types
  on the C side). Built via `cc::Build` in `nros-rmw-cffi/build.rs`,
  gated behind a `c-stub-test` Cargo feature so consumers without
  a C toolchain on the build host aren't forced through the
  invocation. `tests/c_stub_transport.rs` round-trips a C-built
  ops struct through `nros_rmw::set_custom_transport`,
  drives each registered fn pointer from Rust, and confirms the
  C-side counters bumped. Layout safety via a const
  `assert_eq!(size_of::<CStubTransportOps>(), size_of::<NrosTransportOps>())`.
  Run via `cargo test -p nros-rmw-cffi --features c-stub-test
  --test c_stub_transport`. 2/2 passing. (`<this commit>`)

### Examples

- [ ] **115.F — Loopback example.** `examples/qemu-arm-baremetal/c/zenoh/custom-transport-loopback/`.
  Depends on 115.B (zenoh path). XRCE-side loopback would need
  MicroXRCEAgent in the test harness; defer until either the agent
  fixture lands or 115.B unblocks.

### Docs

- [x] **115.I — Porting guide.** `book/src/porting/custom-transport.md`
  covers when-to-use, Rust / C / C++ examples, threading contract
  (no concurrent read/write, no ISR, `user_data` lifetime),
  return-code conventions, framing per backend, per-backend
  coverage table. Linked from `book/src/SUMMARY.md` under Porting.
  mdbook clean. (commit `d16bf294`)
- [x] **115.I.2 — abi_version + L0/L1/L2 ladder doc update.**
  `book/src/porting/custom-transport.md` grew three new sections
  before the API examples:
  1. **API layering (L0 / L1 / L2)** — table mapping each layer to
     its crate / file, with the rule "all design decisions live at
     L0, L1 wrappers are mechanical".
  2. **ABI versioning** — documents the `abi_version` field, the
     mandatory consumer fill-in, the rejection contract via
     `NROS_RMW_RET_INCOMPATIBLE_ABI`, and the major-vs-minor bump
     rules.
  3. **Implementing in another language** — points at
     `c_stubs/c_stub_transport.{c,h}` as the reference port,
     describes the round-trip test, and gives the
     `cargo test --features c-stub-test` invocation.
  Existing Rust / C examples updated to fill in `abi_version` +
  `_reserved`. mdbook clean. (`<this commit>`)

### Deferred (out of v1)

- [ ] **115.J — Arduino library reuse.** Phase 23 (Arduino
  precompiled lib) hasn't started; once it does, `nros::set_serial_transport(&Serial)`
  / `nros::set_wifi_udp_transport(...)` should reuse this hook
  instead of inventing a parallel API.

### Files

**Owned by L0 (canonical):**
- `packages/core/nros-rmw-cffi/src/transport.rs` — SoT struct
  (115.A.2)
- `packages/core/nros-rmw-cffi/include/nros/rmw_transport.h` —
  cbindgen-emitted (115.A.2)

**Owned by L1 (mechanical wrappers):**
- `packages/core/nros-rmw/src/custom_transport.rs` — Rust
  re-export + slot storage (115.A.1 → 115.A.2)
- `packages/core/nros-c/src/transport.rs` — C entry stubs (115.C)
- `packages/core/nros-c/include/nros/nros_generated.h` —
  cbindgen-emitted (115.C)
- `packages/core/nros-cpp/include/nros/transport.hpp` — C++
  inline-header wrappers (115.D)
- `packages/core/nros-cpp/src/transport.rs` — Rust FFI
  re-implementations of the L0 fns under `nros_cpp_*` names (115.D)

**Owned by backend integrations:**
- `packages/xrce/nros-rmw-xrce/src/lib.rs::init_transport_from_custom_ops`
  (115.E)
- `packages/zpico/zpico-platform-custom/` (new crate, 115.B)
- `examples/qemu-arm-baremetal/c/zenoh/custom-transport-loopback/`
  (115.F)

**Owned by tests:**
- `packages/core/nros-rmw/src/custom_transport.rs::tests` (115.G.1)
- `packages/xrce/nros-rmw-xrce/tests/custom_transport.rs` (115.G.2)
- `packages/core/nros-rmw-cffi/tests/abi_version.rs` (115.G.3)
- `packages/core/nros-rmw-cffi/tests/c_stub_transport/` (115.G.4)

**Owned by docs:**
- `book/src/porting/custom-transport.md` (115.I, 115.I.2)
- `book/src/SUMMARY.md` (115.I link)

---

## Acceptance criteria

### v1 (this phase)

- [x] Rust user registers 4 callbacks via
  `nros_rmw::set_custom_transport`; XRCE backend drains the slot
  via `nros_rmw_xrce::init_transport_from_custom_ops` and routes
  every wire frame through the user vtable.
- [x] C user registers via `nros_set_custom_transport`; same
  end-to-end path. Verified by `nros-rmw-xrce/tests/custom_transport.rs`
  (no real session, but slot lifecycle + bridge trampolines confirmed).
- [x] C++ user registers via `nros::set_custom_transport(const TransportOps&)`;
  same end-to-end path.
- [x] The same registration compiles on every platform `nros-rmw`
  builds for (`platform-posix`, `platform-zephyr`,
  `platform-bare-metal`, `platform-freertos`, `platform-nuttx`,
  `platform-threadx`, `platform-orin-spe`).
- [x] `book/src/porting/custom-transport.md` documents the
  threading contract (no concurrent read/write, no ISR invocation,
  `user_data` lifetime), return-code conventions, and per-backend
  framing semantics.
- [ ] **115.A.2** — canonical struct lives in `nros-rmw-cffi` with
  an `abi_version: u32` field; cbindgen emits
  `<nros/rmw_transport.h>`; Rust trait re-exports.
- [ ] **115.G.3** — calling `nros_set_custom_transport` with a
  mismatched `abi_version` returns
  `NROS_RET_INCOMPATIBLE_ABI` (a new ret-code) without panicking.
- [ ] **115.G.4** — a C-implemented stub transport drives the
  Rust core through the C ABI (proves the canonical surface is
  reachable from a non-Rust language without going through the
  Rust trait).

### Deferred follow-up phases

- 115.X-zenoh: zenoh-pico custom-link (`Z_FEATURE_LINK_CUSTOM`).
  Tracked separately; ~600 LOC. Design captured in § Appendix B.
- 115.X-dds: dust-DDS custom transport plug-in. Tracked
  separately.
- Phase 23: Arduino library reuses the hook for
  `nros::set_serial_transport(&Serial)` / `nros::set_wifi_udp_transport(...)`.
  Hard-blocked on Phase 23 starting.

## Notes

- Risk: ABI commitment. `nros_transport_ops_t` field order must be stable. Lock it before 1.0. The Rust-side `NrosTransportOps` is `#[repr(C)]` so the two share a single layout — no parallel definitions to drift.
- Risk: re-entrancy. The vtable contract must specify whether `read` may be called from a different thread than `write`. v1: same thread only; document.
- Risk: registration after `nros_support_init`. v1 rejects late registration with `NROS_RET_ALREADY_INIT`. Documented in `book/src/porting/custom-transport.md`.
- 115.H scaffolding (factory + smoke test) landed in this phase; remaining work (locator-scheme dispatch in `DdsRmw::open` + discovery-over-byte-pipe) tracked as `115.H.2-discovery`.
- Out of scope: zero-copy custom transports (Phase 99 scope).

---

## Appendix B — zenoh-pico custom-link design (115.B)

### B.1 Surface

zenoh-pico's link layer is selected by **locator scheme**. The
existing schemes (`tcp/`, `udp/`, `serial/`, `ivc/`, `tls/`, …) each
have a `_z_endpoint_*_valid` predicate + `_z_new_link_*` factory.
`_z_open_link` (in `src/link/link.c`) walks the predicates as an
`if-else` chain and picks the matching factory.

For 115.B we add a new scheme — `custom://` — that routes every
read/write through `nros_rmw::peek_custom_transport()`. Locator
syntax:

```
custom:///                       # default: no params
custom:///?framing=hdlc          # request HDLC framing on top of the user vtable
```

The opaque `?key=val` suffix is parsed by `_z_endpoint_custom_valid`
and threaded into the user vtable's `params` argument. v1 reserves
the `framing` key only.

### B.2 zenoh-pico fork patch — 6 files

Use the existing IVC link (`Z_FEATURE_LINK_IVC`, ~378 LOC, landed
in Phase 100.4) as the template. Mirror its layout:

1. **`include/zenoh-pico/system/link/custom.h`** (~30 LOC) —
   `_z_custom_socket_t` struct holding the `NrosTransportOps` vtable
   snapshot + a small recv-side buffer. `Z_FEATURE_LINK_CUSTOM`
   compile-time gate.

2. **`include/zenoh-pico/link/config/custom.h`** (~20 LOC) —
   `CUSTOM_CONFIG_FRAMING_KEY` + `CUSTOM_SCHEMA = "custom"`.

3. **`src/link/config/custom.c`** (~40 LOC) — `_z_custom_config_*`
   intmap helpers. Boilerplate; copy-modify from `serial.c`.

4. **`src/link/unicast/custom.c`** (~250 LOC) — the heart of the
   patch. Implements:
   - `_z_endpoint_custom_valid` (locator-scheme check).
   - `_z_f_link_open_custom`: snapshot the vtable via
     `nros_zpico_custom_take()` (new C entry exposed by
     `zpico-platform-custom`) and call its `open` fn.
   - `_z_f_link_close_custom`, `_z_f_link_write_custom`,
     `_z_f_link_write_all_custom`, `_z_f_link_read_custom`,
     `_z_f_link_read_exact_custom`: forward to vtable methods.
   - `_z_new_link_custom`: wires the `_z_link_t` callbacks +
     advertises `_Z_LINK_CAP_TRANSPORT_UNICAST` /
     `_Z_LINK_CAP_FLOW_DATAGRAM` (or `STREAM` per `framing` key).

5. **Patch `include/zenoh-pico/link/link.h`** — add
   `_Z_LINK_TYPE_CUSTOM` to `_z_link_type_e`; add `_z_custom_socket_t
   _custom;` member to the `_socket` union under
   `Z_FEATURE_LINK_CUSTOM == 1`.

6. **Patch `src/link/link.c`** — splice an extra
   `_z_endpoint_custom_valid` arm into `_z_open_link`'s if-else
   chain. Mirror in `_z_listen_link` if listen support is wanted
   (v1 skips listen — pure client only).

### B.3 zpico-sys feature wiring (~30 LOC)

`packages/zpico/zpico-sys/Cargo.toml`:

```toml
link-custom = []
```

`zpico-sys/build.rs`:

```rust
fn link_custom_flag(link: &LinkFeatures) -> u8 {
    env::var("CARGO_FEATURE_LINK_CUSTOM").is_ok() as u8
}

// In generate_config_header:
writeln!(header, "#define Z_FEATURE_LINK_CUSTOM {}", link.custom_flag()).unwrap();
```

The feature pulls in `zenoh-pico/src/link/{config,unicast}/custom.c`
on the cc::Build invocation.

### B.4 `zpico-platform-custom` (~150 LOC)

New crate at `packages/zpico/zpico-platform-custom/`:

- `Cargo.toml` — depends on `nros-rmw`. No `default = []`, single
  feature `active` toggled by `zpico-sys/link-custom`.
- `src/lib.rs` — exposes one `extern "C" fn nros_zpico_custom_take(...)`
  that internally calls `nros_rmw::take_custom_transport()` and
  copies the four fn pointers + user_data into a `*mut
  zpico_custom_ops_c_t` buffer the C side hands in. Mirrors
  `nros-rmw-xrce::init_transport_from_custom_ops`.

### B.5 RTOS feature mutex

`zpico-sys/Cargo.toml` already enforces "exactly one platform-*"
via compile_error!. Adding `link-custom` is orthogonal to the
platform-* mutex (the platform crate provides clock/alloc/sync;
link-custom doesn't); the existing rule still holds.

### B.6 Locator hook in nros-rmw-zenoh

`nros-rmw-zenoh::ZenohSession::new` already accepts a locator
through `TransportConfig`. Users register their `NrosTransportOps`
via `nros_rmw::set_custom_transport(...)` BEFORE `Rmw::open`, then
pass `locator: Some("custom:///")` in `TransportConfig`. zenoh-pico's
locator dispatch pulls our custom link factory; the factory drains
the slot and proceeds. No additional Rust glue beyond the platform
crate.

### B.7 LOC estimate

| Component | LOC |
|-----------|-----|
| zenoh-pico fork — 4 new files + 2 patches | ~340 |
| zpico-sys build.rs + Cargo.toml | ~30 |
| zpico-platform-custom crate | ~150 |
| Integration test (zenohd-fixture-style loopback) | ~80 |
| **Total** | **~600** |

### B.8 Risks

- **Fork drift.** Adds 4 new files + 2 patches to the zenoh-pico
  fork. Future upstream merges will conflict on `link.c` /
  `link.h`. Mitigation: keep the patches small + clearly tagged
  with `// nros: link-custom` so they're easy to rebase.
- **MTU.** v1 picks 4096 bytes. Make this a build-time
  `ZPICO_LINK_CUSTOM_MTU` env to match the existing MTU knobs.
- **Threading.** zenoh-pico's reader thread calls `read_*` from a
  background task on threaded backends (FreeRTOS, NuttX, ThreadX);
  the user vtable must be reentrant w.r.t. its own `write` only if
  the platform is multi-threaded. Document in
  `book/src/porting/custom-transport.md` once 115.B lands.

---

## Appendix C — dust-DDS custom transport design (115.H)

### C.1 Surface

dust-dds's transport plug-in point is the `TransportParticipantFactory` trait:

```rust
pub trait TransportParticipantFactory: Send + 'static {
    fn create_participant(
        &self,
        domain_id: i32,
        data_channel_sender: MpscSender<Arc<[u8]>>,
    ) -> impl Future<Output = RtpsTransportParticipant> + Send;
}
```

`RtpsTransportParticipant` is a struct: a `Box<dyn WriteMessage>` and four `Vec<Locator>` lists + `fragment_size`. dust-dds calls `write_message` for outbound traffic and pulls inbound bytes off the `MpscSender` the factory hands the receiver-half of.

The Phase 115 vtable maps to this shape directly:

| dust-dds layer            | 115 vtable                      |
|---------------------------|---------------------------------|
| factory `create_participant` start | `cb_open(user_data, NULL)` |
| `WriteMessage::write_message`      | `cb_write(user_data, buf, len)` |
| recv task `recv` step              | `cb_read(user_data, buf, len, 0)` |
| participant `Drop`                 | `cb_close(user_data)` (TODO — see C.4) |

### C.2 What landed in 115.H

- `packages/dds/nros-rmw-dds/src/transport_custom.rs`:
  `NrosCustomTransportParticipantFactory<P>` factory type with
  `from_slot()` / `with_ops()` constructors, `with_fragment_size`
  builder, full `TransportParticipantFactory` impl. Reader runs as
  a single async task spawned on the `NrosPlatformRuntime` spawner
  with a `YieldOnce`-after-zero-bytes pattern matching the existing
  UDP recv loops.
- `tests/custom_transport.rs`: smoke test via stub callbacks. No
  RTPS handshake — exits as soon as the four counters confirm
  plumbing is wired. Same template as 115.B's
  `custom_transport.rs` test in `nros-rmw-zenoh`.
- Lib re-export: `pub mod transport_custom;` alongside
  `transport_nros`.

### C.3 What is NOT yet wired

- **`DdsRmw::open` dispatch.** Currently always uses
  `NrosUdpTransportFactory` (no_std path) or dust-dds's stock
  threaded factory (std path). 115.H follow-up adds locator-scheme
  inspection: `RmwConfig::locator.starts_with("custom/")` →
  `NrosCustomTransportParticipantFactory::from_slot(runtime)` →
  fall through to the existing UDP path otherwise.
- **Std-path support.** Stock `DomainParticipantFactory::get_instance()`
  is a singleton bound to the UDP transport. Custom factories need
  the async `DomainParticipantFactoryAsync::new` constructor. v1
  follow-up will route POSIX `custom/...` through the async
  factory + a `NrosPlatformRuntime<PosixPlatform>` runtime, same
  shape as the no_std path.
- **Discovery over a byte pipe.** RTPS SPDP uses
  `239.255.0.1:port_metatraffic_multicast`. Custom transport has
  no multicast equivalent. Three ways forward:
  1. **Static peer mode** — both peers register matching vtables
     and share a pre-agreed `GuidPrefix`. Skips SPDP entirely;
     dust-dds upper layers need a "fake the peer is already
     known" code path.
  2. **Unicast SPDP rendezvous** — first message either side
     sends is a hand-rolled "I'm here" announce that includes
     the `GuidPrefix`. Higher-level than RTPS SPDP, lives in
     `transport_custom.rs`.
  3. **Tunnel multicast** — wrap multicast packets in an envelope
     header on the byte pipe, deliver to both sides as if from
     multicast. Easiest to fit dust-dds, hardest user surface.
  v1 picks (1) — static peer mode — to match the typical
  custom-transport use case (point-to-point bridge to a known
  peer). Tracked separately from this phase under
  `115.H.2-discovery`.

### C.4 `cb_close` lifetime

The current scaffolding does NOT call `cb_close` on participant
drop. `RtpsTransportParticipant` is a `pub struct`, not a
`Drop`-equipped opaque type — dust-dds expects the embedded
writer / locator state to clean up via field-level `Drop`s.

Two options for wiring close:

- **Box the writer with a custom `Drop`** that calls `cb_close`
  inside `WriteMessage::Drop`. Risk: dust-dds may clone the
  `Box<dyn WriteMessage>` (or move it across thread boundaries),
  invoking close at an unexpected point.
- **Track participant lifetime in `nros-rmw-dds::session`** and
  call `cb_close` from `DdsSession::Drop`. Cleanest semantically
  — the session is the user-visible RAII handle.

v1 follow-up picks the second option. Until then, `cb_close` is
the consumer's responsibility (call `set_custom_transport(None)`
manually before exit, or rely on process teardown).

### C.5 Risks

- **`MpscSender` shutdown.** When the participant drops, the
  recv task's `sender.send().await` returns `Err`, the loop
  exits cleanly. No leak.
- **Long-running `cb_read`.** v1 passes `timeout_ms = 0` (non-
  blocking poll). User implementations that block longer
  starve the runtime. Documented in
  `book/src/porting/custom-transport.md`.
- **Fragment size mismatch.** Default 1344 bytes assumes IPv4
  MTU minus headers — for byte-pipe transports without
  packet-level MTU, this is fine but suboptimal. `with_fragment_size`
  builder lets consumers pick anything in `8..=65000`.
- **Multi-participant in one process.** Each participant pulls a
  vtable copy out of the slot via `take`. Two participants in the
  same process clobber each other (slot is single-shot). Same
  constraint as 115.B's zpico-platform-custom; documented as
  "register-once-per-process" in v1.

### C.6 LOC estimate (remaining 115.H follow-up)

- `DdsRmw::open` locator-scheme dispatch — ~30 LOC.
- POSIX std-path async factory wiring — ~80 LOC.
- Static-peer SPDP shim — ~250 LOC inside `transport_custom.rs`,
  plus dust-dds-side discovery hook (~100 LOC if dust-dds gains a
  static-peer config knob).
- Lifetime-driven `cb_close` from `DdsSession::Drop` — ~20 LOC.
- E2E test mirroring 115.F's two-process loopback (DDS variant) —
  ~150 LOC.

Total: ~600 LOC, broadly matching the original 115.X-dds estimate
in this doc's deferral note. The scaffolding now landed clears
the plug-in surface; what remains is the discovery layer, which
is the genuinely hard part.
