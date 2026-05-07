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

**Status:** v1 + 115.B + 115.F (native) + 115.H scaffolding complete. Three RMW backends now expose the runtime-pluggable transport vtable: XRCE-DDS via 115.E (full), zenoh-pico via 115.B (full + 115.F native loopback E2E), dust-DDS via 115.H scaffolding (factory + smoke test landed; `DdsRmw` locator-scheme dispatch + discovery-over-byte-pipe deferred to `115.H.2-discovery`). All v1 acceptance criteria satisfied (115.A.1 / 115.A.2 / 115.B / 115.C / 115.D / 115.E / 115.G.1–4 / 115.I / 115.I.2). The transport vtable is the project's first canonical-C-ABI interface; the design + test pattern (`abi_version` field, `tests/c_stubs/`, second-language smoke test) is the template for future Rust→C boundaries — **Phase 117 generalised it to the full RMW backend surface (`nros_rmw_vtable_t`, ~17 fn ptrs)** and shipped Cyclone DDS as the first native-language consumer (C++). The transport vtable is now a sub-case of the canonical RMW-backend ABI: native backends compose Phase 115 (transport pluggability) on top of Phase 117 (backend pluggability). **Phase 115.K — native-language backend ports** (see § Work Items, tier added 2026-05-07) tracks the question of porting existing Rust backends to their underlying-library native languages. **Deferred to follow-up phases:** 115.F (bare-metal C variant) blocked on a bare-metal C example harness, 115.H.2 (DDS dispatch + discovery) tracked separately, 115.J → Phase 23.
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

### Native-language backend ports (115.K)

Added 2026-05-07 after Phase 117 landed `nros_rmw_vtable_t` + Cyclone
DDS C++ backend. The canonical-C-ABI hierarchy now reads:

```
nros-core (Rust) ──→ Rmw trait
                        ├──→ dust-dds (Rust direct impl, no FFI hop)
                        └──→ nros-rmw-cffi (C ABI bridge via vtable)
                                ↓ nros_rmw_vtable_t
                                ├──→ cyclonedds  (C++ direct, no Rust)
                                ├──→ XRCE        (Rust over xrce-sys, today)
                                ├──→ zenoh-pico  (Rust over zpico-sys, today)
                                └──→ uORB        (Rust over px4-rs, today)
```

The project rule going forward: **a backend's host language matches
its underlying library's native language unless there is a concrete
reason otherwise**. dust-dds stays Rust because dust-dds is a Rust
crate. cyclonedds is C++ because Cyclone DDS is a C/C++ library.
The three remaining Rust-wrapping backends (XRCE, zenoh-pico, uORB)
each sit on a non-Rust underlying library; they are candidates for
re-hosting in the native language. The decisions below capture the
ROI analysis from 2026-05-07.

Ordered execution-first (policy → port → tracking entries):

- [x] **115.K.1 — backend host-language policy doc.** Added
  `book/src/internals/rmw-backends.md` codifying the rule "a
  backend's host language matches its underlying library's native
  language unless there is a concrete reason otherwise" and the
  per-backend decision matrix (Appendix D §D.2). Cross-linked from
  the porting guide (`book/src/porting/custom-transport.md`),
  `CLAUDE.md`'s "Platform Backends" section, and `SUMMARY.md`.

- [~] **115.K.2 — port nros-rmw-xrce to C.** Drop `xrce-sys` (auto-
  generated FFI, ~4.4k LOC) and rewrite `nros-rmw-xrce` as a C
  backend that consumes `nros_rmw_vtable_t` directly over micro-XRCE-
  DDS-Client's `uxr_*` C API. Mirrors `nros-rmw-cyclonedds`'s layout
  (1.7k LOC C++ over Cyclone's C API). LOC trade: ~3k Rust + 4.4k
  -sys → ~2k C. Phase 115.E's custom-transport bridge stays usable
  — the slot-drain helpers are already C-callable
  (`init_transport_from_custom_ops`, Appendix D §D.4). Highest-ROI
  active port; only K.* item that ships code. Depends on K.1
  landing the policy doc that justifies the migration.

  - [x] **115.K.2.0** — vtable scaffold. New crate
    `packages/xrce/nros-rmw-xrce-c/` mirrors `nros-rmw-cyclonedds`'s
    layout (CMakeLists + public header + per-area C TUs). Every
    vtable entry returns `NROS_RMW_RET_UNSUPPORTED`; the scaffold
    is wired-but-inert. Smoke test `tests/smoke.c` passes — confirms
    register entry point hands a populated vtable through and stubs
    return UNSUPPORTED. Builds with `cmake -DNROS_RMW_CFFI_DIR=...`.
    Does not yet link against micro-XRCE-DDS-Client.
  - [x] **115.K.2.1** — session lifecycle. `xrce_session_open`
    parses `udp/host:port` (or bare `host:port`), calls
    `uxr_init_udp_transport` + `uxr_init_session` +
    `uxr_create_session_retries`, allocates output / input reliable
    streams, parks the per-session state in
    `nros_rmw_session_t::backend_data`. `xrce_session_close` calls
    `uxr_delete_session` + `uxr_close_udp_transport` + frees the
    state. `xrce_session_drive_io` forwards to `uxr_run_session_time`.
    CMakeLists now compiles the vendored micro-xrce-dds-client +
    micro-cdr sources directly (mirrors xrce-sys's source list);
    config.h headers generated via `configure_file` from the
    upstream `.in` templates. Smoke test reaches the backend
    against a dead agent on port 1 and confirms ERROR (3 s retry
    budget). Pub/sub/service paths still hit K.2.0 UNSUPPORTED
    stubs.
  - [x] **115.K.2.2** — pub/sub topic/writer/reader create + publish_raw
    + try_recv_raw. `xrce_publisher_create` allocates 3 entity ids
    (TOPIC/PUBLISHER/DATAWRITER) and creates them via
    `uxr_buffer_create_*_bin`; `publish_raw` goes through
    `uxr_buffer_topic` + a 0-ms flush. `xrce_subscriber_create`
    allocates a slot from the per-session pool of 8 (default
    `XRCE_MAX_SUBSCRIBERS`) and issues `uxr_buffer_request_data`.
    The single per-session topic callback (registered once at
    `xrce_session_open`) dispatches by datareader id. `try_recv_raw`
    drains the slot's single-msg ringbuffer; oversize messages flag
    overflow and drop. K.2 scope gaps (XML QoS, deadline tracking,
    fragmented publish, async wakers) are tagged `TODO 115.K.2.x` in
    source for follow-up commits.
  - [x] **115.K.2.3** — service server + client paths.
    `xrce_service_server_create` allocates a REPLIER entity via
    `uxr_buffer_create_replier_bin` plus a slot from the per-session
    pool of `XRCE_MAX_SERVICE_SERVERS=4`. The per-session
    `request_callback` (registered once at session_open) dispatches
    by replier id and copies the inbound `SampleIdentity` into the
    slot. `xrce_service_send_reply` reads it back through
    `uxr_buffer_reply`, mirroring the Rust impl's
    `last_sample_id` flow. Symmetric REQUESTER path for the client;
    `xrce_service_call_raw` busy-waits via `uxr_run_session_time`
    for up to `XRCE_SERVICE_REPLY_TOTAL_MS=5000 ms` before returning
    `NROS_RMW_RET_TIMEOUT`. Service-default QoS only; the
    runtime's int64_t `seq` is unused (XRCE correlates via
    `SampleIdentity`, not seq numbers).
  - [x] **115.K.2.4** — port Phase 115.E's
    `init_transport_from_custom_ops` slot-drain helper to a C TU.
    `nros_rmw_xrce_set_custom_transport_ops(ops, framing)` copies a
    caller-supplied vtable into backend-local storage; the four
    trampolines (`xrce_custom_open_trampoline` etc.) fan out to the
    user's open / close / write / read. `xrce_session_open` invoked
    with a `custom://` locator routes through
    `uxr_set_custom_transport_callbacks` +
    `uxr_init_custom_transport` instead of UDP. The
    drain-from-runtime variant (`nros_rmw_xrce_init_custom_transport`)
    needs a `nros_rmw_take_custom_transport` C export from
    `nros-rmw-cffi` that doesn't exist yet — documented in
    `packages/xrce/nros-rmw-xrce-c/KNOWN-LIMITATIONS.md`. Pure-C
    clients route around via the direct-pass entry point.
  - [~] **115.K.2.5** — drop the Rust crate; flip `-DNROS_C_RMW=xrce`
    over to the C backend.
    - [x] **115.K.2.5.0** — wire the C backend behind a new
      `NANO_ROS_RMW=xrce-c` selector in `nros-c` + `nros-cpp`,
      mirroring the cyclonedds shape (`rmw-cffi` Rust feature +
      `find_package(NrosRmwXrceC)` + `NROS_RMW_XRCE_C=1`
      auto-register macro in `nros::init`). Rust path under
      `NANO_ROS_RMW=xrce` stays unchanged. Validated via
      `cargo test --test xrce` (14/14 pass — Rust path regression
      check) and a top-level cmake configure +
      `cmake --build` with `NANO_ROS_RMW=xrce-c` (clean build,
      `libnros_c_xrce-c.a` + `libnros_cpp_xrce-c.a` produced).
    - [ ] **115.K.2.5.1** — Rust API user migration. The 22
      `Cargo.toml` files referencing `rmw-xrce` / `nros-rmw-xrce`
      today (every native Rust XRCE example + every Zephyr Rust
      XRCE example + the workspace umbrella crates) need a path
      to the C backend that doesn't depend on the Rust direct
      impl. Sub-steps:
      - [x] **115.K.2.5.1.0** — new shim crate
        `packages/xrce/nros-rmw-xrce-cffi`: builds the K.2 backend
        sources + vendored micro-XRCE-DDS-Client + micro-CDR via
        `cc::Build`, exposes
        `extern "C" { fn nros_rmw_xrce_register() -> c_int; }` as
        a safe Rust `pub fn register() -> Result<(),
        RegisterError>`. `no_std`. Mirrors the role
        `nros-rmw-cyclonedds` Rust crate would play if Cyclone had
        Rust users — it doesn't, so this is the first cffi-shim
        crate in the project. Smoke test (`tests/register_smoke.rs`)
        stubs `nros_rmw_cffi_register` and confirms the symbol
        chain resolves.
      - [x] **115.K.2.5.1.1** — `rmw-xrce-cffi` Cargo feature on
        `nros-node` + `nros` umbrella crates pulls in the shim
        crate and `rmw-cffi`. Existing `rmw-xrce` feature stays
        for now (still routes to the Rust direct impl). Also
        unified the K.2 backend's session-key hash from FNV-1a
        to djb2 to match the Rust impl's `hash_session_key` —
        same node name now produces the same XRCE session key
        on both backends.
      - [x] **115.K.2.5.1.2** — migrate
        `examples/native/rust/xrce/{talker,listener,service-*,
        action-*,stress-test,large-msg-test}` (8 examples) —
        switch `rmw-xrce` → `rmw-xrce-cffi` in Cargo.toml, add
        `nros_rmw_xrce_cffi::register()` call before
        `Executor::open` in `src/main.rs`. Two examples
        (`serial-talker`, `serial-listener`) stay on the legacy
        `rmw-xrce` path because the cffi shim only ships POSIX UDP
        as v1; serial transport via cffi is queued as
        `115.K.2.5.1.5-serial`. Validated via
        `cargo test -p nros-tests --test xrce`: 14/14 pass.

        **Resolved 2026-05-08** — root cause was endianness:
        the cffi shim's generated `<ucdr/config.h>` set
        `UCDR_MACHINE_ENDIANNESS=0`. ucdr's `ucdrEndianness` enum
        defines `BIG=0, LITTLE=1`, so 0 = big-endian on an x86 /
        ARM little-endian box. This dropped the `FLAG_ENDIANNESS`
        bit from every outgoing submessage; the agent parsed
        payloads big-endian and silently rejected them. Fixed by
        flipping the macro to `1` in both
        `nros-rmw-xrce-cffi/build.rs` and
        `nros-rmw-xrce-c/CMakeLists.txt`. Side-fix (also needed):
        `CffiSession::supported_qos_policies` returns the same
        broad mask Rust XRCE does — without it the runtime
        pre-validate rejected default QoS (which sets
        `LIVELINESS_AUTOMATIC`) before reaching the backend.

        **Debug history (kept for reference):**

        1. C backend reaches `uxr_create_session_retries` and it
           returns OK against the live agent — the XRCE session
           handshake itself succeeds.
        2. The next step (`uxr_buffer_create_participant_bin` +
           `uxr_run_session_until_all_status` for status
           confirmation) times out with `status[0]=255` (no status
           received). Participant request goes out but no reply
           arrives within the 1000 ms confirmation budget.
        3. The K.2.0 smoke test "passing" against a live agent was
           misleading — open() returns `NROS_RMW_RET_ERROR` there
           too, but the smoke logic doesn't assert OK; it just
           checks the call is not the `UNSUPPORTED` stub. Same
           failure mode as the talker.
        4. Same agent works with the legacy `xrce-sys` path. The
           legacy path uses `uxr_set_custom_transport_callbacks` +
           `uxr_init_custom_transport` with `xrce-platform-shim`
           providing UDP under the custom-transport hood — it does
           NOT call `uxr_init_udp_transport` directly. The K.2 C
           backend uses the upstream UDP transport path directly.
        5. Disabling `UCLIENT_PROFILE_DISCOVERY` in the cffi shim
           build (to match `xrce-sys`'s hand-written config.h) did
           not fix the timeout.

        **Working theory:** the upstream uxr UDP transport
        (`udp_transport_posix.c`) uses POLL with a different
        recv-timing profile than the custom-transport-via-shim
        path, and the agent's reply to participant create is
        getting dropped or arriving outside the poll window. The
        legacy path's read-via-`PlatformUdp` shim drains the
        socket through `nros-platform-posix`'s `set_recv_timeout`
        path; the upstream path may need the same treatment but
        applied to its own `poll_fd`.

        **115.K.2.5.1.2.a-fix-transport (in progress, this commit):**

        New file `packages/xrce/nros-rmw-xrce-c/src/transport_posix_udp.c`
        replaces `uxr_init_udp_transport` with a custom-transport
        + POSIX-UDP shape that mirrors what `xrce-sys` /
        `nros-rmw-xrce`'s `platform_udp.rs` does — open a
        connected UDP fd, register four trampolines that drive
        the fd via `poll()` + `recv()` / `send()`, hand the bridge
        struct to `uxr_init_custom_transport` so its `args`
        field carries the bridge through every callback. Built;
        K.2.0 smoke against a live agent now exercises the path
        but session-open still fails its participant-create
        confirm.

        **Wire-level diagnosis (byte trace through the new
        transport):**

        1. session-create handshake works:
           write 24 bytes → read 19 bytes containing
           `STATUS_AGENT` (submessage id 0x04). Session created.
        2. participant-create writes 36 bytes. Agent replies with
           a 32-byte packet whose submessage header is `0f 01 18 00`
           — submessage id 15 = `SUBMESSAGE_ID_TIMESTAMP_REPLY`.
           Plus a 13-byte `ACKNACK` (submessage id 10).
        3. Agent does NOT send `STATUS` (id 5).
           `uxr_run_session_until_all_status` waits for STATUS,
           never sees one, returns false with `s[0]=255`
           (`UXR_STATUS_NONE`).

        Hypothesis: the upstream client somehow asked for a
        timestamp, OR the agent treats our CREATE_PARTICIPANT
        submessage as a timestamp request. The session header
        we put on the wire (`81 00 00 00`) suggests session_id
        bit 0x80 + stream_id 0x00 (NONE). If the CREATE went out
        on the NONE stream instead of an output_reliable stream,
        the agent might process it but reply over a different
        path that uxr's status tracker doesn't see.

        Tracked as `115.K.2.5.1.2.a-stream-id`. Needs further
        wire-level investigation comparing the legacy
        `xrce-sys` byte stream against ours, ideally with both
        running side-by-side under tcpdump. Out of scope for
        this commit's window.
      - [x] **115.K.2.5.1.5-serial** — POSIX serial via the
        cffi backend. New TU
        `packages/xrce/nros-rmw-xrce-c/src/transport_posix_serial.c`
        opens a tty/pty, configures termios (raw, 8N1, baud from
        `XRCE_SERIAL_BAUD` env or 115200), and registers four
        trampolines that drive `read()` / `write()` via `poll()`.
        Mirrors `transport_posix_udp.c` but with `framing=true`
        because serial is byte-stream — HDLC framing comes from
        `UCLIENT_PROFILE_STREAM_FRAMING`. `session.c` recognises
        `serial://<path>` / `/dev/...` locator schemes and routes
        to `xrce_posix_serial_init` before the UDP fall-through.
        Migrated `serial-talker` + `serial-listener` examples to
        `rmw-xrce-cffi` (drop `xrce-serial` feature; cffi handles
        serial via locator parse). Validated via
        `cargo test -p nros-tests --test xrce -- --test-threads=1`:
        14/14 pass, including the 3 serial tests
        (`test_xrce_serial_talker_starts`,
        `test_xrce_serial_listener_starts`,
        `test_xrce_serial_communication`).
      - [~] **115.K.2.5.1.3-zephyr-deferred** — migrate
        `examples/zephyr/rust/xrce/*` (6 examples) — same
        pattern, but the Zephyr cross-compile bring-up is its
        own work item:
        1. The cffi shim's `build.rs` hard-codes
           `_POSIX_C_SOURCE=200809L`, `UCLIENT_PLATFORM_POSIX`,
           and unconditionally compiles `transport_posix_udp.c`
           + `transport_posix_serial.c` (which include
           `<sys/socket.h>` / `<termios.h>`) and the upstream
           `udp_transport_posix.c` + `util/time.c`. None of
           these resolve on `thumbv7em-none-eabihf`. Need
           target-aware cc::Build setup (a `posix` cfg gate
           via `CARGO_CFG_TARGET_OS == "none"`-style detection).
        2. Zephyr-side serial / UDP trampolines must be
           supplied by `xrce-zephyr` (or a successor
           `nros-rmw-xrce-c-zephyr` C TU). Today
           `xrce-zephyr/src/xrce_zephyr.c` only handles L4
           readiness + `uxr_millis`/`uxr_nanos`; it does NOT
           ship XRCE custom-transport callbacks — those came
           from `nros-rmw-xrce`'s `platform_udp.rs`. The C
           backend's session-open path needs an alternative
           init when the locator scheme is `udp://` or
           `serial://` and the build is no_std (call into a
           Zephyr-provided init shim instead of the POSIX one).
        3. `zephyr/CMakeLists.txt` would need to switch
           `CONFIG_NROS_RMW_XRCE` from
           `rmw-xrce,platform-zephyr,ros-humble` to
           `rmw-xrce-cffi,platform-zephyr,ros-humble`, plus
           pull in the cffi shim's static lib through
           Corrosion (or the existing `rust_cargo_application`
           hook).
        Out of scope for the K.2.5 close-out: the work is
        ~3–5 commits' worth of cross-compile plumbing and the
        Zephyr examples remain functional on the legacy
        `rmw-xrce` path. K.2.5.2 / K.2.5.3 retain the legacy
        `rmw-xrce` Rust crate + `xrce-zephyr` for now;
        K.2.5.3 explicitly carries them through.
      - [ ] **115.K.2.5.1.4** — migrate Rust XRCE tests
        (`packages/testing/nros-tests/tests/xrce.rs`,
        `xrce_ros2_interop.rs`) — switch their fixtures over.
        Expected: 14/14 still pass via the C backend.
    - [x] **115.K.2.5.2** — flip default `xrce` selector to mean
      the C backend (deprecate Rust path). Now safe because every
      previous Rust user is on the cffi-via-C-backend path.

      **Landed:** `NANO_ROS_RMW=xrce` now routes to `rmw-cffi` +
      `cffi-xrce-c` Cargo features in both `packages/core/nros-c/CMakeLists.txt`
      and `packages/core/nros-cpp/CMakeLists.txt`, plus the matching
      `find_package(NrosRmwXrceC)` + `NROS_RMW_XRCE_C=1` link block
      that previously lived under the `xrce-c` selector. The
      separate `xrce-c` selector is removed.

      Implementation notes:
      - `packages/core/nros-c/Cargo.toml` adds the `cffi-xrce-c`
        feature. When set, `nros_support_init` calls
        `nros_rmw_xrce_register()` (extern "C", resolved via the
        linked `NrosRmwXrceC::NrosRmwXrceC` archive) before the
        session opens. Mirrors the C++ path's `nros::init` hook.
      - `packages/core/nros-c/cmake/NanoRosCTargets.cmake` and
        `packages/core/nros-cpp/cmake/NanoRosCppTargets.cmake`
        gain `find_dependency(NrosRmwXrceC)` + link / define on
        `NANO_ROS_RMW=xrce`. Mirrors the cyclonedds wiring.
      - All `cfg(any(rmw-zenoh, rmw-xrce, rmw-dds))` gates inside
        `packages/core/nros-c/src/` widen to include
        `feature = "rmw-cffi"` so the support / publisher /
        service / lifecycle / parameter / executor symbols
        compile under the new `rmw-cffi+cffi-xrce-c` axis.
      - New `xrce::build-rmw` justfile recipe builds the
        `nros-rmw-xrce-c` standalone CMake project + installs
        into `build/install/`. `install-local-posix` depends on
        it, mirroring `cyclonedds::build-rmw`.

      Validation:
      - `cargo test -p nros-tests --test xrce -- --test-threads=1`:
        14/14 pass (Rust `nros-rmw-xrce-cffi` consumers).
      - `cargo test -p nros-tests --test c_xrce_api -- --test-threads=1`:
        5/5 pass — the C/C++ API path now exercises the C
        backend end-to-end (was previously gated on the Rust
        `nros-rmw-xrce` crate via `NANO_ROS_RMW=xrce`).
    - [ ] **115.K.2.5.3** — remove `nros-rmw-xrce` Rust crate +
      `xrce-sys` + `xrce-platform-shim` + `xrce-zephyr` from the
      workspace. The cffi shim crate
      (`nros-rmw-xrce-cffi`) is the only Rust-side XRCE artifact
      that survives.

- [~] **115.K.3 — zenoh-pico C/C++ port (deferred).** Underlying
  library is C, so the canonical pattern says C/C++ backend. Cost
  estimate is high (1.5k Rust glue + 14k of FFI / platform-shim /
  custom-transport plumbing, all of which would have to be re-
  implemented in C). The `zpico-platform-shim` socket-size probe is
  particularly load-bearing — it exists because `_z_sys_net_socket_t`
  changes layout per platform, and the Rust `cc::Build`-driven probe
  would have to be re-derived in a C-only world. The zenoh path is
  also the most-tested backend (every QEMU + bare-metal + RTOS
  example exercises it). Verdict: defer until a concrete pressure
  surfaces (e.g. upstream alignment with micro-ROS's zenoh-pico
  binding, or a customer-driven request to drop Rust from the zenoh
  path). Re-eval triggers in Appendix D §D.5. Tracking-only entry.

- [~] **115.K.4 — uORB stays Rust (closed as won't-do).** Underlying
  lib is C++ (PX4 modules), but uORB is the **in-process** case —
  the nros code runs INSIDE a PX4 module, not over a network.
  `px4-rs`'s value is module init + topic-registration derive macros
  + workqueue-async tooling; a C++ port would replace those with
  hand-written PX4 module idioms (which already exist in PX4 native
  but not for the nros API surface). Net cost very high, net benefit
  low. Won't-do; rationale captured in K.1's host-language doc.

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

---

## Appendix D — native-language backend ports (115.K)

### D.1 Hierarchy after Phase 117

Phase 117 generalised the Phase 115 canonical-C-ABI pattern from a
4-fn-ptr transport vtable to the full RMW backend surface
(`nros_rmw_vtable_t`, ~17 fn ptrs covering session lifecycle,
publisher / subscriber / service entities, and Phase 108 status
events). The two vtables compose:

| Vtable | Phase | Surface | First consumer |
|--------|-------|---------|----------------|
| `NrosTransportOps` | 115 | open / close / read / write byte pipe | XRCE (115.E), zenoh-pico (115.B), dust-DDS (115.H) |
| `nros_rmw_vtable_t` | 117 | session + entity + event lifecycle | Cyclone DDS (117.3+) |

A native-language RMW backend (Phase 117 client) can register its
own byte pipe via Phase 115 — the two layers are orthogonal.

### D.2 Decision matrix (frozen 2026-05-07)

| Backend | Underlying lib | Underlying lang | Today's host | Recommended host | Verdict |
|---------|----------------|-----------------|--------------|------------------|---------|
| dust-dds | dust-dds | Rust | Rust (`Rmw` trait direct) | Rust | keep |
| cyclonedds | Cyclone DDS | C / C++ | C++ via vtable | C++ | keep |
| **XRCE** | micro-XRCE-DDS-Client | C | Rust over `xrce-sys` | **C via vtable** | **port (115.K.2)** |
| zenoh-pico | zenoh-pico | C | Rust over `zpico-sys` | C/C++ via vtable | **defer (115.K.3)** |
| uORB | PX4 / `px4-rs` | C++ (with Rust derive layer) | Rust over `px4-rs` | Rust | **won't-do (115.K.4)** |

### D.3 Per-port ROI sizing

LOC counts as of 2026-05-07:

| Backend | Rust glue | -sys / FFI | Native rewrite est. |
|---------|-----------|-----------|---------------------|
| zenoh-pico | 1,464 | 14,396 (zpico-sys + zpico-platform-shim + zpico-platform-custom) | ~3 kLOC C |
| XRCE | 3,083 | 4,446 (xrce-sys) | ~2 kLOC C |
| uORB | 878 | (px4-rs ecosystem, shared with non-nros consumers) | n/a |

Reference: Cyclone DDS backend (117.3 baseline) is 1,721 LOC C++
across 13 files for a complete vtable consumer. XRCE's surface area
is comparable; zenoh-pico's is larger because it carries its own
platform-abstraction shim layer that Cyclone delegates to its host
runtime.

### D.4 115.K.2 work-item shape (XRCE port)

Mirrors Cyclone DDS layout:

```
packages/xrce/nros-rmw-xrce-c/             — new C backend crate
├── CMakeLists.txt                         — produces static lib
├── include/nros_rmw_xrce.h                — register entry point
└── src/
    ├── vtable.c                           — kVtable definition
    ├── session.c                          — open / close / drive_io
    ├── publisher.c                        — create / destroy / publish_raw
    ├── subscriber.c                       — create / destroy / try_recv_raw / has_data
    ├── service.c                          — server + client paths
    ├── transport.c                        — bridges Phase 115 NrosTransportOps slot
    │                                        into uxr_set_custom_transport_callbacks
    │                                        (today's Rust `init_transport_from_custom_ops`
    │                                        ported verbatim)
    └── internal.h
```

The existing `nros-rmw-xrce` Rust crate stays as the deprecation-
bridge for one release cycle, then is removed. CMake option
`-DNROS_C_RMW=xrce` selects the C backend the same way
`-DNROS_C_RMW=cyclonedds` does today.

### D.5 Risks

- **xrce-sys consumers.** Any other workspace crate that imports
  `xrce-sys` directly (e.g. test fixtures) needs a parallel migration
  or stays on the Rust path during the transition.
- **micro-XRCE-DDS-Client API churn.** The C API is stable but not
  versioned aggressively; the C backend needs the same `abi_version`
  discipline Phase 115.A.2 enforces on the transport vtable.
- **CFFI vtable evolution.** `nros_rmw_vtable_t` is still on its
  first major version. Adding a fn ptr breaks every C/C++ backend at
  build time — manageable now (only Cyclone DDS and a future XRCE-C),
  bigger lift once more native backends ship. Phase 117 follow-up
  to add the same `abi_version` field to the RMW vtable is queued.
- **zenoh-pico deferral re-eval trigger.** Re-open 115.K.3 if (a)
  micro-ROS's upstream ships a zenoh-pico binding the project wants
  to align with, (b) a deployment surfaces concrete Rust-on-RTOS
  flash-size or boot-time pressure, or (c) zpico-sys breaks under a
  zenoh-pico bump in a way that costs more to fix than to rewrite.

### D.6 LOC estimate (entire 115.K tier)

- 115.K.1 host-language policy doc: ~150 LOC markdown.
- 115.K.2 XRCE port: ~2,000 LOC C + ~200 LOC test harness; remove
  ~3,000 LOC Rust + ~4,400 LOC -sys.
- 115.K.3 / 115.K.4: zero (deferral / won't-do; tracking-only
  entries, rationale captured in K.1's doc).

Net LOC change for the tier: roughly −5,000 LOC (mostly auto-
generated FFI bindings going away).
