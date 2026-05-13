# RMW Backends — Host-Language Policy

This page documents which language each RMW backend is implemented in,
and the rule that decides it. The matrix was originally frozen
2026-05-07 under Phase 115.K.1; the L tier (Phase 115.L.7 + L.8,
landed 2026-05-12) collapsed the public RMW surface so every backend
now reaches the runtime via the same `nros_rmw_vtable_t` bridge. The
underlying implementations still ship in whichever language their
upstream library prefers, but consumers never see that — they only
see the C vtable.

## The rule (post-115.L)

> **Every backend installs itself via the `nros_rmw_vtable_t`
> bridge.** The underlying library's host language is an
> implementation detail of the per-backend `-cffi` shim; it does
> not appear on the consumer surface.

The original rule (a backend's host language matches its underlying
library's native language unless overridden) is still how we pick
the inside of each `-cffi` shim — but the shim itself is uniform:
a small Rust or C++ TU that fills in a vtable and calls
`nros_rmw_cffi_register(&vtable)` once at startup.

## Hierarchy

```
nros-core (Rust) ──→ Rmw trait (internal; bridged by RustBackendAdapter<R>)
                        └──→ nros-rmw-cffi   (C ABI bridge, registry)
                                ↓ nros_rmw_vtable_t  (~17 fn ptrs)
                                ├──→ nros-rmw-zenoh    (wraps Rust nros-rmw-zenoh)
                                ├──→ nros-rmw-dds      (wraps Rust nros-rmw-dds)
                                ├──→ nros-rmw-xrce-cffi     (links C nros-rmw-xrce)
                                ├──→ nros-rmw-cyclonedds    (C++ direct, no Rust)
                                └──→ nros-rmw-uorb      (C++ direct, no Rust)
```

The shims are the canonical consumer surface. Public Cargo features
on `nros` / `nros-c` / `nros-cpp` (`rmw-{zenoh,dds,xrce}-cffi`,
`cffi-{zenoh-cffi,dds-cffi,xrce-c}`) all route through the same
`nros_rmw_vtable_t` runtime. The pre-L.7 direct-Rust-trait features
(`rmw-zenoh`, `rmw-dds`, `rmw-xrce`, `rmw-uorb`) are gone.

Any language with stable C-ABI interop (C, C++, Zig, Rust,
Go-via-cgo, Python-via-ctypes…) can implement a backend by filling
in the vtable and calling `nros_rmw_cffi_register(&vtable)` once at
startup.

## Decision matrix (post-115.L)

| Backend | Underlying lib | Underlying lang | Shim crate | Verdict |
|---------|----------------|-----------------|------------|---------|
| dust-DDS | dust-dds | Rust | `nros-rmw-dds` (Rust → vtable via `RustBackendAdapter<DdsRmw>`) | keep |
| Cyclone DDS | Cyclone DDS | C / C++ | `nros-rmw-cyclonedds` (C++ direct vtable) | keep |
| XRCE | micro-XRCE-DDS-Client | C | `nros-rmw-xrce-cffi` (Rust shim over the C `nros-rmw-xrce` static lib; 115.K.2 ported) | keep |
| zenoh-pico | zenoh-pico | C | `nros-rmw-zenoh` (Rust → vtable via `RustBackendAdapter<ZenohRmw>`) | keep |
| uORB | PX4 module SDK | C++ | `nros-rmw-uorb` (C++ direct vtable; 115.K.4 port replaces legacy `nros-rmw-uorb` Rust crate) | keep |

### Rust-backend cffi shape

For backends whose upstream library is Rust (dust-DDS, zenoh-pico)
the cffi shim ships as a tiny crate that calls
`RustBackendAdapter::<UnderlyingRmw>::register()`. The adapter
monomorphizes a static `nros_rmw_vtable_t` over the Rust `Rmw`
trait impl and installs it into the C registry. Consumer code never
sees the trait surface; it only sees the vtable.

The legacy direct-Rust-trait crates (`nros-rmw-zenoh`,
`nros-rmw-dds`, `nros-rmw-xrce`) stay in the workspace as
internal-only implementation libs of these shims. They have no
public Cargo feature reaching them after Phase 115.L.7.

### C-/C++-backend cffi shape

For backends whose upstream library is C/C++ (Cyclone DDS, uORB,
XRCE) the cffi shim is a standalone CMake project that builds a
static C/C++ library and registers a `nros_rmw_vtable_t` at startup
via `nros_rmw_cffi_register`. No `RustBackendAdapter` is involved.
The Rust runtime sees these via the same registry; the
`NANO_ROS_RMW=<name>` CMake selector flips a build-time macro that
ensures the register call is wired into `nros::init`.

## When to revisit

This matrix is a snapshot. Update it when any backend's situation
changes:

- A new backend lands → add a row + shim-crate + verdict.
- An existing backend's underlying library changes language (e.g.
  zenoh-pico ships a Rust port upstream) → swap the shim shape
  (Rust adapter vs. C/C++ direct) but the vtable bridge stays.

The rule stays. Only per-backend verdicts and shim shapes move.

## Registry + naming (Phase 104.B.2)

`nros-rmw-cffi` holds a fixed-size named registry of backend
vtables. Each backend registers under a canonical name at
process startup:

| Backend | Name | Registered by |
|---|---|---|
| zenoh-pico | `"zenoh"` | `nros_rmw_zenoh_register()` (auto-ctor on POSIX) |
| dust-DDS | `"dds"` | `nros_rmw_dds_register()` (auto-ctor on POSIX) |
| micro-XRCE-DDS-Client | `"xrce"` | `nros_rmw_xrce_register()` (C ctor on POSIX) |
| Cyclone DDS | `"cyclonedds"` | `nros::init` hook (C++ explicit call) |
| uORB | `"uorb"` (future) | TBD |

### Naming policy

- **Lowercase ASCII** identifying the protocol / wire format.
  Not the transport variant — `"xrce"` covers both XRCE-UDP and
  XRCE-serial; the transport is selected via the locator (`udp/...`
  vs `serial:/dev/...`).
- **Stable across releases.** Renaming a registered name is a
  breaking change for bridge code that selects backends by string.
- **No `"default"` for new backends.** The string `"default"` is
  reserved for the legacy single-arg `nros_rmw_cffi_register`
  shim — single-backend builds where the backend's specific name
  doesn't matter.

### Capacity

Registry size: `NROS_RMW_MAX_BACKENDS` build-time env var consumed
by `nros-rmw-cffi/build.rs`. Default 8. Range [1, 64]. Set lower
on Cortex-M0+ (where each slot's ~40 B costs); set higher for
bridge nodes with 4+ backends. Hitting the cap = subsequent
`nros_rmw_cffi_register_named` returns `NROS_RMW_RET_ERROR`.

### Default-backend convention

`Executor::open` and any `create_node` call without an explicit
`.rmw(name)` selector use the **first-registered backend** — the
nano-ros equivalent of ROS 2's `RMW_IMPLEMENTATION`. Single-
backend binaries with one auto-registering backend Just Work
without user code mentioning the backend's name. Build-time
selection happens at:

- **Cargo feature**: `--features cffi-zenoh-cffi` (Rust users).
- **CMake**: `NANO_ROS_RMW=zenoh cmake ...` (C/C++ users).

No runtime env-var override; selection is fixed at link time
(RTOS-friendly, matches our static-link world).

### Symbol-survival mechanism

Backend register symbols must survive linker dead-strip. Three
levers, all currently active without `--whole-archive`:

1. **Rust ctor:** `#[unsafe(link_section = ".init_array")] #[used]
   static AUTO_REGISTER_CTOR` in each backend's `src/lib.rs`.
   `#[used]` is the load-bearing attribute — tells rustc the static
   is reachable from outside Rust, suppressing dead-strip.
2. **C ctor:** `__attribute__((constructor)) static void
   nros_rmw_<name>_register_ctor`. Same survival via
   `.init_array` walk by libc startup.
3. **Explicit reference:** `nros_support_init` (C path) and
   `nros::init` (C++ path) call `nros_rmw_<name>_register()`
   directly. The reference brings in the register fn, which
   references the vtable, which keeps everything alive.

Bare-metal targets without `.init_array` walking (RTIC,
FreeRTOS, some NuttX configs) rely on (3) — the explicit call
from `nros_support_init` / `nros::init`. Future Phase 104.B.6
will codify a `nano_ros_link_rmw` CMake stub that emits a
register call for bare-metal builds when (3) is bypassed (pure-
Rust binaries on no_std targets).

## See also

- [Phase 115 roadmap doc](../../../docs/roadmap/phase-115-runtime-transport-vtable.md)
  — Appendix D carries LOC sizing, port shapes, and risk notes.
- [Custom Transport porting guide](../porting/custom-transport.md) —
  how the Phase 115 transport vtable composes with the Phase 117
  RMW vtable.
- `packages/dds/nros-rmw-cyclonedds/` — reference layout for the
  C++ vtable consumer (Phase 117).
- `packages/px4/nros-rmw-uorb/` — reference layout for the
  C++ vtable consumer with PX4 SDK integration (Phase 115.K.4).
