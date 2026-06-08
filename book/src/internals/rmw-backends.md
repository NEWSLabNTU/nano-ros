# RMW Backends — Host-Language Policy

This page documents which language each RMW backend is implemented in,
and the rule that decides it. The matrix was originally frozen
2026-05-07; the L tier (+ L.8,
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
                                ├──→ nros-rmw-xrce-cffi     (links C nros-rmw-xrce)
                                ├──→ nros-rmw-cyclonedds    (C++ direct, no Rust)
                                └──→ nros-rmw-uorb      (C++ direct, no Rust)
```

The shims are the canonical consumer surface. Public Cargo features
on `nros` / `nros-c` / `nros-cpp` (`rmw-{zenoh,xrce}-cffi`,
`cffi-{zenoh-cffi,xrce-c}`) all route through the same
`nros_rmw_vtable_t` runtime. The pre-L.7 direct-Rust-trait features
(`rmw-zenoh`, `rmw-xrce`, `rmw-uorb`) are gone.

**Phase 169 (2026-05-19) — dust-dds retired.** The `nros-rmw-dds`
Rust shim and the `dust-dds` upstream Rust DDS implementation
have been removed (Phase 169.4). Cyclone DDS is the sole DDS
backend; the `nros-rmw-cyclonedds` shim registers under its
canonical name `"cyclonedds"` only. The previous `"dds"` generic
slot is **not** aliased — callers always select Cyclone by its
specific name (`NROS_RMW=cyclonedds`, `target_link_libraries(...
NanoRos::Rmw::cyclonedds)`, etc.).

Any language with stable C-ABI interop (C, C++, Zig, Rust,
Go-via-cgo, Python-via-ctypes…) can implement a backend by filling
in the vtable and calling `nros_rmw_cffi_register(&vtable)` once at
startup.

## Decision matrix (post-115.L, updated by Phase 171)

| Backend | Underlying lib | Underlying lang | Shim crate | Verdict |
|---------|----------------|-----------------|------------|---------|
| Cyclone DDS | Cyclone DDS | C / C++ | `nros-rmw-cyclonedds` (C++ direct vtable; canonical DDS backend) | keep |
| XRCE | micro-XRCE-DDS-Client | C | `nros-rmw-xrce-cffi` (Rust shim over the C `nros-rmw-xrce` static lib; 115.K.2 ported) | keep |
| zenoh-pico | zenoh-pico | C | `nros-rmw-zenoh` (Rust → vtable via `RustBackendAdapter<ZenohRmw>`) | keep |
| uORB | PX4 module SDK | C++ | `nros-rmw-uorb` (C++ direct vtable; 115.K.4 port replaces legacy `nros-rmw-uorb` Rust crate) | keep |

Dust-DDS was retired in Phase 169 (2026-05-19) after repeated
bring-up failures on embedded targets. It is intentionally absent
from the active backend table; Cyclone DDS now fills the DDS slot.

### Rust-backend cffi shape

For backends whose upstream library is Rust (zenoh-pico) the cffi
shim ships as a tiny crate that calls
`RustBackendAdapter::<UnderlyingRmw>::register()`. The adapter
monomorphizes a static `nros_rmw_vtable_t` over the Rust `Rmw`
trait impl and installs it into the C registry. Consumer code never
sees the trait surface; it only sees the vtable.

The legacy direct-Rust-trait crates (`nros-rmw-zenoh`,
`nros-rmw-xrce`) stay in the workspace as internal-only
implementation libs of these shims. They have no public Cargo
feature reaching them after.

### C-/C++-backend cffi shape

For backends whose upstream library is C/C++ (Cyclone DDS, uORB,
XRCE) the cffi shim is a standalone CMake project that builds a
static C/C++ library and registers a `nros_rmw_vtable_t` at startup
via `nros_rmw_cffi_register`. No `RustBackendAdapter` is involved.
The Rust runtime sees these via the same registry; the
`NANO_ROS_RMW=<name>` CMake selector flips a build-time macro that
ensures the register call is wired into `nros::init`.

### Cyclone DDS: runtime type introspection (Phase 212.K.7)

The Cyclone DDS shim **does not** require per-type backend code on the
generated msg crate. Phase 212.K.7 inverted the original design where
each msg crate carried an optional `cyclonedds` Cargo feature plus a
Cyclone-specific descriptor sidecar.

In the runtime-introspection design every generated msg crate is
purely the wire-format data type (`#[derive]`d struct + a tiny `impl
nros_serdes::Message` exposing `const TYPE_NAME` + `const FIELDS`).
On the first typed `create_publisher<M>` / `create_subscription<M>`
for a given `M`, the `nros-rmw-cyclonedds` shim walks the static
field schema, builds a Cyclone `ddsi_sertype` via Cyclone's
dynamic-type C API, and caches the pointer in a bounded
`heapless::FnvIndexMap<u64, NonNull<ddsi_sertype>, MAX_TYPES>`
guarded by a platform-selected mutex. Subsequent uses of the same
`M` are hash-map hits.

End state: no `<msg-pkg>/cyclonedds` Cargo feature anywhere, no
per-msg-pkg backend code, no codegen branching on the active RMW.
The shape matches upstream rclcpp's introspection typesupport + rclrs's
plain `<pkg> = "*"` consumer manifest.

Sizing knob: `NROS_CYCLONEDDS_MAX_TYPES` (default 32), wired through
the existing `nros-sizes` build probe (same pattern as
`EXECUTOR_OPAQUE_U64S`). See section 212.K.7 of
[`docs/roadmap/phase-212-ux-cargo-native-and-file-consolidation.md`](../../../docs/roadmap/phase-212-ux-cargo-native-and-file-consolidation.md)
for the work-item ledger.

## When to revisit

This matrix is a snapshot. Update it when any backend's situation
changes:

- A new backend lands → add a row + shim-crate + verdict.
- An existing backend's underlying library changes language (e.g.
  zenoh-pico ships a Rust port upstream) → swap the shim shape
  (Rust adapter vs. C/C++ direct) but the vtable bridge stays.

The rule stays. Only per-backend verdicts and shim shapes move.

## Registry + naming

`nros-rmw-cffi` holds a fixed-size named registry of backend
vtables. Each backend registers under a canonical name at
process startup:

| Backend | Name | Registered by |
|---|---|---|
| zenoh-pico | `"zenoh"` | `nros_rmw_zenoh_register()` (auto-ctor on POSIX) |
| micro-XRCE-DDS-Client | `"xrce"` | `nros_rmw_xrce_register()` (C ctor on POSIX) |
| Cyclone DDS | `"cyclonedds"` | `nros_rmw_cyclonedds_register()` (Phase 169.5 — canonical DDS backend; no generic `"dds"` alias) |
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
without user code mentioning the backend's name.

The user-facing knob is a **declared, language-agnostic, per-deploy
value** (`system.toml` `[system].rmw` / `[deploy.<t>].rmw`, or a
CLI/build flag) that the toolchain **lowers** to each language's
native link mechanism. The Cargo feature / shim dep and the CMake
cache var below are those *lowering targets* — what the build uses,
not how a user picks a backend (see
[RFC-0031](../../../docs/design/0031-rmw-selection-and-lowering.md)):

- **Cargo (Rust)**: the declared RMW lowers to the `nros` `rmw-<x>`
  feature plus the matching `nros-rmw-<x>` shim dep in the consumer's
  `[dependencies]`. Linking the shim crate is what registers the
  backend.
- **CMake**: `cmake -DNANO_ROS_RMW=zenoh ...` (C/C++ users). The
  `nano_ros_link_rmw(... RMW zenoh)` helper auto-generates the
  per-target `nros_app_register_backends()` strong stub.

No runtime env-var override; selection is fixed at link time
(RTOS-friendly, matches our static-link world).

### Symbol-survival mechanism

Backend register symbols must survive linker dead-strip. Four
mechanisms, layered:

1. **`linkme` distributed-slice** (/ 128.H.2) — each
   backend contributes an `RMW_INIT_ENTRIES` entry through the
   `nros_rmw_register_backend!` macro. `nros_support_init` /
   `Executor::open` walks the slice and calls each entry. Canonical
   on Linux / macOS / Windows / POSIX. Macro expands to a no-op on
   RTOS targets where `linkme` can't recognise the section (NuttX,
   Zephyr, ESP-IDF, FreeRTOS bare-metal).
2. **Rust ctor** (legacy fallback): `#[unsafe(link_section =
   ".init_array")] #[used] static AUTO_REGISTER_CTOR`. `#[used]`
   keeps rustc from dead-stripping.
3. **C ctor** (legacy fallback): `__attribute__((constructor))
   static void nros_rmw_<name>_register_ctor`. Same survival via
   `.init_array` walk by libc startup.
4. **CMake strong stub** (landed): the
   `nano_ros_link_rmw(<target> RMW <name>)` helper at
   `cmake/NanoRosLink.cmake:62-117` emits an auto-generated TU
   per target that defines a strong `nros_app_register_backends()`
   calling every linked RMW's `nros_rmw_<name>_register()`. The
   weak default in `libnros_c_weak_stubs.a` is overridden. This is
   the canonical path on every RTOS where `linkme` can't survive.
5. **Explicit user call** (Rust no_std bridges): `nros_rmw_<name>::register()`
   from `main()` — drags the rlib's CGU into the binary so the
   linkme entry is reachable. See `examples/bridges/native-rust-zenoh-to-dds/`.

Bare-metal + RTOS targets that don't run `.init_array` rely on
(4). Pure-Rust no_std binaries with multiple backends rely on (5).
POSIX builds get (1) + (2) + (3) for free.

### Ctor ordering

POSIX `.init_array` runs ctors in **link order**, not in any
user-controlled sequence. When multiple backends auto-register
in one binary, the **first to fire owns the default slot** —
the one selected by `Executor::open()` / `nros::init()` with no
`.rmw(name)` argument. The order is reproducible per link
graph but not portable across linkers (lld vs. mold vs. gold)
or build configs (LTO can reorder via `--print-icf-sections`
collapse) and must not be relied on for correctness.

**Disambiguation is the user's job in multi-backend binaries.**
Use the named entry points:

- Rust: `Executor::open_with_rmw("zenoh", &cfg)` for the
  primary session; `node_builder("name").rmw("cyclonedds").build()`
  for additional Nodes.
- C: `nros_node_init_ex` with `nros_node_options_t.rmw_name`
  set.
- C++: `nros::Executor::open_with_rmw(...)` and
  `nros::NodeBuilder::rmw(...)` mirror the Rust API (Phase
  104.C.9).

The `examples/bridges/native-rust-zenoh-to-cyclonedds/` demo shows
the pattern end-to-end: both Zenoh and Cyclone DDS backend ctors fire
at lib-load (so the registry has both `"zenoh"` and `"cyclonedds"`
slots populated), and `open_with_rmw("zenoh", ...)` plus
`node_builder("egress").rmw("cyclonedds")` pin each session to its
intended backend without depending on link-order luck.

**Single-backend builds** keep the legacy ergonomics — only
one ctor fires, the default-slot convention picks it up, and
no user-visible name is ever required. The cost of naming is
paid only when multiple backends coexist.

## Real-time budget per backend

The poll loop's worst-case execution time is dominated by the
backend's transport drain. Bridge users summing
`bridge_wcet = Σ poll_i + Σ dispatch_j` need each backend's
contribution; this table captures the current best-effort
estimates from per-backend microbenchmarks
(`packages/testing/nros-bench/wcet-cycles-qemu/`,
`packages/testing/nros-bench/wake-latency-cortex-m3/`) +
heap-usage stats from `cargo build --release` symbol
dumps.

| Backend | `poll_wcet_us` | Buffer-pool size | Notes |
|---|---|---|---|
| **zenoh-pico** (`nros-rmw-zenoh`) | ~50–200 µs nominal on Cortex-M3 (FreeRTOS QEMU); P99 ≤ 1 ms under 100 Hz pub load | `Z_BATCH_UNICAST_SIZE` (default 6500 B/peer) + 4 KB per subscription buffered ring | Wake-cb collapses idle wait to kernel `xSemaphore` post — sub-poll-period latency when transport notifies. POSIX cv-wait path same shape, ~1 µs notify-to-dispatch. |
| **XRCE-DDS** (`nros-rmw-xrce-cffi`) | ~100–500 µs per `uxr_run_session_time` on POSIX; agent-round-trip dominates over local poll. Bare-metal targets pay the same poll cost. | `STREAM_HISTORY` (4) × `UXR_CONFIG_UDP_TRANSPORT_MTU` (512 B default) ≈ 2 KB/stream; one input + one output stream per session | Poll-only — `set_wake_callback` slot is NULL; spin_once cv-wait still wakes on its deadline. Agent does the reliable-retransmit accounting; client adds ~10 µs per stream per tick. |
| **Cyclone DDS** (`nros-rmw-cyclonedds`) | ~150–600 µs on POSIX; C++ listener callback latency depends on Cyclone's reader-cache scan. | Cyclone's RTPS history per the DDS QoS `History.depth` (default 10) + Cyclone's own DDSI buffer pool (~32 KB default) | Listener-side `set_wake_callback` wiring is follow-up — today the C++ vtable sets the slot NULL. Memory footprint dominated by Cyclone itself, not the nano-ros shim. |

**Bridge users:** sum the `poll_wcet_us` for every backend the
bridge process opens, then add per-callback dispatch budget
(typically <10 µs for the executor's arena dispatch + the
user callback's own work). A `bridge_picas_priority` regression
test (blocked on the PiCAS dispatcher) will eventually pin a
bar to this table.

Per-backend `README.md` files live at
`packages/{zpico/nros-rmw-zenoh,dds/nros-rmw-cyclonedds,xrce/nros-rmw-xrce-cffi}/README.md`
(when present); reach out to the backend's maintainer for
fresh microbench numbers on a different target class than
the ones above.

## See also

- [roadmap doc](../../../docs/roadmap/archived/phase-115-runtime-transport-vtable.md)
  — Appendix D carries LOC sizing, port shapes, and risk notes.
- [Custom Transport porting guide](../porting/custom-transport.md) —
  how the transport vtable composes with the RMW vtable.
- `packages/dds/nros-rmw-cyclonedds/` — reference layout for the
  C++ vtable consumer.
- `packages/px4/nros-rmw-uorb/` — reference layout for the
  C++ vtable consumer with PX4 SDK integration.
