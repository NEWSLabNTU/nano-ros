# RMW Backends — Host-Language Policy

This page documents which language each RMW backend is implemented in,
and the rule that decides it. The decision matrix is **frozen
2026-05-07** under Phase 115.K.1; future backends inherit the same rule.

## The rule

> **A backend's host language matches its underlying library's native
> language unless there is a concrete reason otherwise.**

Concrete reasons that override the default:

- The underlying library is a thin shim around a Rust ecosystem
  (e.g. `px4-rs` for uORB) — staying in that ecosystem keeps derive
  macros and async tooling first-class.
- A port costs more than its expected benefit (existing Rust glue
  is well-tested, FFI surface is auto-generated, downstream pressure
  is absent).

## Hierarchy

```
nros-core (Rust) ──→ Rmw trait
                        ├──→ dust-dds        (Rust direct impl, no FFI hop)
                        └──→ nros-rmw-cffi   (C ABI bridge)
                                ↓ nros_rmw_vtable_t  (Phase 117 — ~17 fn ptrs)
                                ├──→ cyclonedds       (C++ direct, no Rust)
                                ├──→ XRCE             (Rust over xrce-sys today;
                                │                       C native after 115.K.2)
                                ├──→ zenoh-pico       (Rust over zpico-sys; deferred)
                                └──→ uORB             (Rust over px4-rs; won't-do)
```

`nros_rmw_vtable_t` is the canonical RMW backend surface. Any
language with stable C-ABI interop (C, C++, Zig, Rust, Go-via-cgo,
Python-via-ctypes…) can implement a backend by filling in the vtable
and calling `nros_rmw_cffi_register(&vtable)` once at startup.

The Phase 115 transport vtable (`NrosTransportOps`) is a sub-case:
backends consume it on top of their own RMW vtable when they want
runtime-pluggable byte pipes.

## Decision matrix

| Backend | Underlying lib | Underlying lang | Host today | Host policy | Verdict |
|---------|----------------|-----------------|------------|-------------|---------|
| dust-dds | dust-dds | Rust | Rust (`Rmw` trait direct) | Rust | keep |
| cyclonedds | Cyclone DDS | C / C++ | C++ via vtable | C++ | keep |
| **XRCE** | micro-XRCE-DDS-Client | C | Rust over `xrce-sys` | **C via vtable** | **port (115.K.2)** |
| zenoh-pico | zenoh-pico | C | Rust over `zpico-sys` | C/C++ via vtable | defer (115.K.3) |
| uORB | PX4 / `px4-rs` | C++ via Rust derive layer | Rust over `px4-rs` | Rust | won't-do (115.K.4) |

### dust-dds — keep Rust

dust-dds is a Rust crate. The natural backend implements the `Rmw`
trait directly with no FFI hop. No reason to introduce a C ABI here.

### Cyclone DDS — keep C++

Cyclone DDS ships a C/C++ public API. The Phase 117 backend
(`packages/dds/nros-rmw-cyclonedds`) is a 1.7 kLOC C++ static lib
that implements `nros_rmw_vtable_t` over Cyclone's C entities. No
Rust glue, no `-sys` crate, no FFI marshalling.

### XRCE — port to C (115.K.2)

micro-XRCE-DDS-Client is a small, stable C library. The micro-ROS
reference impl is C. Today's `nros-rmw-xrce` is ~3 kLOC of Rust
sitting on ~4.4 kLOC of auto-generated `xrce-sys` bindings; a C
backend implementing `nros_rmw_vtable_t` directly over `uxr_*`
would be ~2 kLOC C, mirroring the Cyclone DDS layout.

ROI is high enough that the port is queued as Phase 115.K.2. It is
the only active code item in the K tier — the rest are policy or
tracking-only.

### zenoh-pico — defer (115.K.3)

zenoh-pico is a C library. By the rule, the canonical backend is
C/C++. But the cost-benefit doesn't pencil out today:

- The Rust glue is small (1.5 kLOC `nros-rmw-zenoh`); the bulk of
  the dep tree (~14 kLOC across `zpico-sys` + `zpico-platform-shim`
  + `zpico-platform-custom`) is auto-generated FFI plus a load-
  bearing platform-abstraction layer.
- `zpico-platform-shim` does compile-time per-platform socket-size
  probing via `cc::Build`. Replicating that in a pure-C backend
  means re-deriving the probe — non-trivial.
- The zenoh path is the most-tested backend in the project. Every
  QEMU + bare-metal + RTOS example exercises it. A rewrite would
  reset the verification clock.

Re-eval triggers (any one re-opens K.3):

1. Upstream micro-ROS ships a zenoh-pico binding the project wants
   to align with.
2. A deployment surfaces concrete Rust-on-RTOS flash-size or
   boot-time pressure that a C rewrite would meaningfully cut.
3. `zpico-sys` breaks under a zenoh-pico bump in a way that costs
   more to fix than to rewrite.

Until then: tracking-only entry, no rewrite work.

### uORB — won't-do (115.K.4)

uORB sits in a different category from the network RMWs. It runs
**in-process** inside a PX4 module — there is no transport layer,
no discovery, no wire format. The host-language rule pulls toward
C++ (PX4 modules are C++), but the override applies:

- `px4-rs` provides PX4 module init macros, topic-registration
  derive macros, and async workqueue integration. Those idioms are
  the value `nros-rmw-uorb` brings; without them the user is back
  to writing native PX4 modules.
- A C++ backend would have to re-implement the derive-macro
  ergonomics in a C++ shape that does not exist yet upstream.
- nros-rmw-uorb is 878 LOC. The thinnest backend in the project.

Net cost very high, net benefit low. Closed as won't-do; this
file is the canonical location of that decision.

## When to revisit

This matrix is a snapshot. Update it when any backend's situation
changes:

- A new backend lands → add a row + verdict + rationale.
- An existing backend's underlying library changes language (e.g.
  zenoh-pico ships a Rust port upstream) → re-derive the verdict.
- A re-eval trigger fires for a deferred entry (115.K.3) → flip
  the verdict and open a phase to do the work.

The rule itself stays. Only per-backend verdicts move.

## See also

- [Phase 115 roadmap doc](../../../docs/roadmap/phase-115-runtime-transport-vtable.md)
  — Appendix D carries LOC sizing, port shapes, and risk notes.
- [Custom Transport porting guide](../porting/custom-transport.md) —
  how the Phase 115 transport vtable composes with the Phase 117
  RMW vtable.
- `packages/dds/nros-rmw-cyclonedds/` — reference layout for the
  C++ vtable consumer (Phase 117).
