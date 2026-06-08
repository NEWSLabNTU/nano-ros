---
rfc: 0026
title: "Example directory layout"
status: Stable
since: 2026-02
last-reviewed: 2026-06
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# Example directory layout

> **Revised 2026-06.** This RFC originally proposed a depth-4
> `platform/language/rmw/use-case` hierarchy. That layout was **superseded**:
> Phase 118 + 168 **collapsed the RMW dimension out of the path** (RMW is now a
> build-time choice, not a directory). The current canonical shape is below; the
> depth-4 history is in the Changelog.

## Canonical shape

```
examples/<platform>/<language>/<example>/
```

RMW is selected **at build time**, not encoded in the path:

- Rust → a cargo feature lowered from the declared RMW (RFC-0031), `default = ["rmw-zenoh"]`.
- C / C++ → `-DNANO_ROS_RMW=<rmw>`.
- Zephyr → a `prj-<rmw>.conf` Kconfig overlay.

So one `examples/zephyr/rust/talker/` builds against zenoh, xrce, or cyclonedds —
there are no `<rmw>/` siblings. Phase 168.6.C deleted the legacy
`<plat>/<lang>/<rmw>/<case>/` triples on Zephyr.

Each example directory is a **standalone copy-out template** (RFC, per its own
"Examples = Standalone Projects" rules): its own `Cargo.toml` + `.cargo/config.toml`
+ `CMakeLists.txt`, no workspace walk-up.

## Sibling categories

- `examples/<plat>/<lang>/<example>/` — the canonical per-platform examples.
- `examples/bridges/<name>/` — cross-RMW gateways (link ≥2 backends).
- `examples/templates/<name>/` — multi-platform copy-out recipes (Pattern A workspaces, etc.).
- `examples/workspaces/<lang>/` — multi-node workspace examples (Node pkg + Bringup
  pkg + Entry pkg; see RFC-0024/0025).

Variant naming uses a **suffix** form so variants sort with their peers:
`talker-rtic`, `service-client-async`, `talker-rtic-mixed`.

## Carve-outs

- `examples/zephyr/cpp/cyclonedds/talker-aemv8r/` — one-board-one-RMW reference,
  intentionally **not** collapsed.
- Deliberately empty cells (no harness exists): bare-metal `{c,cpp}` (no hosted
  RTOS startup/heap/libc), and `px4/{c,rust}` (PX4 is uORB-only, C++-only port).

## Authority

The authoritative matrix of which `<plat>/<lang>/<rmw>` triples exist lives in
`examples/README.md` ("Coverage matrix" + "Intentionally empty cells"). Phase 118
lint blocks untriaged cells. Non-example binaries (tests/benches/smokes) live
under `packages/testing/{nros-tests/bins,nros-bench,nros-smoke}/`, not `examples/`.

## Changelog

- 2026-06 — Revised to the collapsed `<plat>/<lang>/<example>/` shape (RMW is a
  build-time choice). Added bridges/templates/workspaces siblings + carve-outs.
- 2026-02 — Original proposal: depth-4 `platform/language/rmw/use-case` hierarchy
  with per-RMW directories. Superseded by the Phase 118 + 168 collapse.
