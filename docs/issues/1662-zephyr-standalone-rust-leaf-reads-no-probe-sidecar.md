---
id: 1662
title: "A standalone Zephyr Rust leaf now PROBES, and nothing on its west road reads the sidecar — its pools stay at the Kconfig / crate defaults"
status: open
type: tech-debt
area: [build, tooling, examples]
severity: low
found: 2026-10-03
related: [1603, 1407, 1265, 1061, rfc-0100, phase-470]
---

## What

Issue 1603's Zephyr half split each `examples/zephyr/rust/*` leaf into a
host-buildable NODE package (`<leaf>/node/`) and the thin west image half, so
the metadata probe answers for all six (`node/metadata/<component>.json` --
measured, e.g. the listener's sidecar holds its one subscription). That closes
the "no producer" half of 1603's acceptance. It does not close the other half,
"pools derived": nothing on a standalone Zephyr Rust leaf's road reads that
sidecar.

## Evidence

`examples/zephyr/rust/listener`, `native_sim/native/64`, zenoh, west-built
through the repo's own `build-one` recipe before and after the split:

| | `.bss` of `zephyr.exe` | `ZPICO_MAX_SUBSCRIBERS` in the build |
| --- | --- | --- |
| before (no sidecar) | 857,144 | 8 (Kconfig) |
| after (sidecar: 1 subscription) | 857,144 | 8 (Kconfig) |

The image is byte-for-byte the same size: the probe's facts reach no knob.

## Why

The road is `rust_cargo_application()` (zephyr-lang-rust) over the leaf's own
`Cargo.toml`, with the Zephyr module's knob resolver supplying the C-side
values from Kconfig. A Rust Zephyr leaf calls no `nano_ros_entry()`, so
neither the cmake entity-facts fold (`nros_record_entity_facts`, which reads a
MODEL) nor the sizing-descriptor naming that issue 1407 gave a west ENTRY
reaches it, and the cargo-leaf road that DOES read a leaf's probe
(`leaf_entity_env`, written into `build/<image>/nros-cargo.toml`) is not the
road west runs. CLAUDE.md already records the standalone Zephyr leaf as
reaching no descriptor producer; this is the same gap one input earlier, now
that the input exists.

## Direction (not decided)

Feed the leaf's probe-derived facts to the west road the way issue 1407 fed a
west entry's descriptor: `nros sync` already writes the leaf's facts for the
cargo road, so the question is the carrier into `rust_cargo_application()`'s
cargo invocation (which inherits no `set(ENV{})`, issue 0460) and into the C
lane's knob resolver. Acceptance: the listener above built with its sidecar
sizes `ZPICO_MAX_SUBSCRIBERS` (and the executor tables) from the one
subscription, and an edit adding a subscription to `node/src/lib.rs` moves
them with no declaration touched.
