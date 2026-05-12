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
                                ├──→ nros-rmw-zenoh-cffi    (wraps Rust nros-rmw-zenoh)
                                ├──→ nros-rmw-dds-cffi      (wraps Rust nros-rmw-dds)
                                ├──→ nros-rmw-xrce-cffi     (links C nros-rmw-xrce-c)
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
| dust-DDS | dust-dds | Rust | `nros-rmw-dds-cffi` (Rust → vtable via `RustBackendAdapter<DdsRmw>`) | keep |
| Cyclone DDS | Cyclone DDS | C / C++ | `nros-rmw-cyclonedds` (C++ direct vtable) | keep |
| XRCE | micro-XRCE-DDS-Client | C | `nros-rmw-xrce-cffi` (Rust shim over the C `nros-rmw-xrce-c` static lib; 115.K.2 ported) | keep |
| zenoh-pico | zenoh-pico | C | `nros-rmw-zenoh-cffi` (Rust → vtable via `RustBackendAdapter<ZenohRmw>`) | keep |
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
