---
rfc: 0031
title: "RMW backend selection and lowering"
status: Stable
since: 2026-06
last-reviewed: 2026-06
implements-tracked-by: [phase-227]
supersedes: []
superseded-by: null
---

# RFC-0031 — RMW backend selection and lowering

## Summary

A nano-ros build selects exactly one RMW backend per binary. The selection is a
**declared, language-agnostic config value** (in `system.toml`, or a CLI/build
flag), which the toolchain **lowers** to each language's native build mechanism
(a Rust cargo feature, a CMake cache var). The cargo feature is the *lowering
target*, not the user-facing knob — which resolves a long-standing documentation
contradiction (feature-vs-dependency) and an example inconsistency
(zenoh/xrce wired one way, cyclonedds another).

## Motivation

The repo carried two conflicting stories. `nros` has `rmw-zenoh` / `rmw-xrce` /
`rmw-cyclonedds` cargo features (deleted in Phase 128.C.3, **re-added** in Phase
214.S for parity); some docs say "select by feature," others say "select by
dependency, **not** features on `nros`." Examples were inconsistent: native
talker pulled `nros-rmw-zenoh`/`-xrce` as project-level optional deps but routed
cyclonedds through `nros/rmw-cyclonedds`. And a cargo feature is **Rust-only** —
it cannot be the canonical knob for a C/C++ project, which selects via CMake.
A single, cross-language selection model was needed.

## Design

### Scope: per-deploy, not per-node

A binary links the cffi runtime plus exactly **one** registered backend vtable,
so **RMW is a property of the deploy target / binary**. All nodes in a deploy
inherit it. In-process multi-RMW exists only via an explicit `[[bridge]]`
(RFC-0009), which opens additional sessions deliberately.

### Declared home, lowered per language

| Scope | User declares RMW in | Lowered by toolchain to |
|---|---|---|
| Workspace | `system.toml` `[system] rmw` (+ `[deploy.<t>] rmw` override) | Rust node pkg → cargo feature; C/C++ node pkg → `-DNANO_ROS_RMW`; C++ entry → CMake cache |
| Single-node, with `system.toml` | `[system] rmw` | same lowering |
| Single-node, no `system.toml` | CLI/build flag, else default | same lowering |

The Rust `nros` `rmw-<x>` feature and the CMake `NANO_ROS_RMW` var are the
**lowering targets** the toolchain sets (or a user sets manually as an override).
They are documented as *the mechanism the build uses*, never as *the way you pick
a backend*.

### Precedence (highest wins)

1. CLI / build flag — `nros … --rmw <x>`, `-DNANO_ROS_RMW=<x>`.
2. `system.toml` `[deploy.<target>] rmw`.
3. `system.toml` `[system] rmw`.
4. Default — `zenoh`.

### Common runtime

`nros` is always built with `rmw-cffi`, so `ConcreteSession = CffiSession`. The
selected backend crate, once linked, **self-registers** through the
`nros_rmw_vtable_t` C ABI via the link-time `RMW_INIT_ENTRIES` registry; the
walker resolves it at `Executor::open`. Selection therefore reduces to *"link the
chosen backend,"* which every language's lowering achieves.

### CycloneDDS exception

cyclonedds is not pure-cargo linkable — its register symbol lives in the
C++/CMake backend. Cyclone selection always routes through the CMake/Corrosion
build path (RFC-0005 / Phase 175), even for an otherwise-Rust binary. The
*declaration* is identical (`rmw = "cyclonedds"`); only the lowering differs.

### Consumer wiring (examples) — unified via the umbrella + facade force-link

**All three backends route through the `nros` umbrella feature**
(`rmw-<x> = ["nros/rmw-<x>"]`), with **no `register()` call in user `main.rs`**.
The mechanism (Phase 227.3, reopened 2026-06-09):

- `nros`'s `platform-*` / `ros-*` / `std` / `safety-e2e` / `link-tls` features
  **forward** to the optional backend via `?/` (re-adding the Phase-104.A
  forwarding — `?` keeps it inert for non-selected backends, so the bridge model
  is unaffected). This was safe to restore because 104.A only dropped forwarding
  as collateral of bridge decoupling, and Phase 214.S brought the optional
  backend deps back.
- `nros` carries `#[used] __FORCE_LINK_{ZENOH,XRCE}` statics (gated on the rmw
  feature + a non-bare-metal platform) that reference the backend's `register`,
  keeping its `RMW_INIT_ENTRIES` self-register section in the link graph. This is
  **cycle-free in the facade** because `nros-rmw-zenoh`/`-xrce` do not depend on
  `nros` — an earlier draft wrongly placed it in `nros-node` (where a cycle was
  possible) and concluded "won't-do"; the facade avoids that entirely.
- cyclonedds keeps its `nros-node` `__FORCE_LINK_CYCLONEDDS_SYS` keep-alive (its
  register is a C++ symbol in a leaf `-sys` crate) — same *user-facing* shape.

**Exceptions that stay explicit:** bare-metal / RTOS targets where `linkme` is
unsupported keep an explicit `register()` (supplied from config via the C
`nros_app_register_backends()` stub or a Kconfig overlay); and bridge nodes link
multiple backends and select per-session (`open_multi`).

*Verified 2026-06-09:* the native talker builds + runs on zenoh and xrce through
the umbrella with no `register()` (both reach the transport layer = the backend
self-registered); the old explicit-register build fails identically → no
regression.

## Alternatives considered

- **Cargo feature as the canonical knob.** Rejected: Rust-only; cannot express
  C/C++ selection; leaks a build-system detail into the user model.
- **Backend crate as a direct dependency (the project-dep pattern).** Workable
  for Rust but still Rust-only and still not the *declared* config; kept only as
  the lowered form, not the surface.
- **Per-node RMW.** Rejected: a binary links one backend; per-node backends would
  require multiple processes or the bridge, which the `[[bridge]]` path already
  covers explicitly.

## Open questions / gaps (tracked by phase-227)

- Add `rmw` resolution for single-node from `system.toml` / flag (today single-node
  Rust uses the cargo feature directly).
- Converge examples so zenoh / xrce / cyclonedds all lower uniformly from the
  declared value.
- Make `nros new --rmw <x>` actually template the scaffold (today it only prints
  a "next steps" banner).
- Sync the contradictory book pages (`user-guide/rmw-backends.md`,
  `internals/rmw-backends.md` say "not by features"; `reference/build-commands.md`,
  `porting/custom-platform.md` show the bare feature).

## Changelog

- 2026-06 — created; resolves the feature-vs-dependency contradiction by making
  RMW a declared-and-lowered, per-deploy, language-agnostic selection.
