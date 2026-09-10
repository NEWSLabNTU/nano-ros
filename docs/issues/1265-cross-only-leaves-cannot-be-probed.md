---
id: 1265
title: "The metadata probe cannot run for a cross-only leaf, so esp32 and mps2 examples must DECLARE their entities by hand"
status: open
type: tech-debt
area: [tooling, build]
related: [1061, 1142, 0827, 0939, rfc-0098, phase-445]
---

## What

`nros sync` learns what a Rust component creates by building a METADATA PROBE
of it for the HOST and reading `<leaf>/metadata/<component>.json`. Pool sizes
(`ZPICO_MAX_SUBSCRIBERS`, the executor arena, …) are derived from that file
(issue 0827).

A leaf the host cannot build gets `<component>.json.unprobeable` instead. Two
shapes cause it (`leaf_entity_env.rs`):

- a foreign `[build] target` with `[unstable] build-std` — the esp32-c3 leaves
  (`riscv32imc-unknown-none-elf`, `build-std = ["core", "alloc"]`);
- a board crate with no host build — the mps2-an385 bare-metal leaves.

For those, issue 1061's fix has the leaf DECLARE its entities
(`[package.metadata.nros.component] entities = [...]`, same grammar as
`nano_ros_node_register(... ENTITIES ...)`).

## Why it is a workaround, not a fix

The declaration is cross-checked against the probe **only where the probe
runs**. On exactly the boards that need it, nothing checks it, so it can go
stale the moment someone adds a subscription and forgets the list. A stale
declaration under-sizes a pool, and on esp32-c3 a pool sized wrong is not a
graceful error: `.stack` is the linker leftover after `.bss` (issues 0190,
1052), and `ZPICO_MAX_*` sizes fixed C arrays where too small is a
registration failure at boot.

It also puts a node's contract in `Cargo.toml`, which RFC-0098 reserves for
Rust-toolchain facts. Phase-445 W3 moves the declaration to `system.toml`
`[[component]]` (RFC-0098 D8) — a better home for the same workaround, not the
end of it.

## Fix direction (not decided)

Read the entities from the artifact that IS built — the cross-compiled leaf —
rather than from a host rebuild of it. Candidates, none measured yet:

- the component emits its entity table into a dedicated, `#[used]` link
  section (the `__NROS_SIZE_*` / `__NROS_LU_SZ_*` markers already use this
  shape — `nros-sizes-build::extract_sizes` reads symbol SIZES from an rlib
  without running anything), and sync reads it from the target `.rlib`;
- a host shim of the board crate, so the probe links on the host.

Acceptance: an esp32 and an mps2 leaf with NO declared entities get the same
derived pool sizes as today, and adding a subscription to either changes them
without touching a declaration.
