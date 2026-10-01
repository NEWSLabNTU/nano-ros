---
id: 1604
title: "A C entry drops the model's contract monitor rows: the C pack renders no
  `nros_cpp_install_monitors` block, so a C image with declared contracts runs
  unmonitored and says nothing"
status: open
type: bug
area: codegen, cli
severity: medium
related: [phase-462, phase-474, rfc-0052, rfc-0091]
---

## What happens

phase-462 W1 (RFC-0052) bakes the model's contract monitor rows — one per
contracted publisher (`min_rate_hz`, node-path `max_latency_ms`) and one per
age contract (`max_age_ms`) — into the generated C/C++ entry, and installs them
on each executor through `nros_cpp_install_monitors` before the first node is
created. It did this on the **C++ road only**.

Measured on `origin/main` (2026-10-01), `cmd/codegen.rs::run_entry`:

* the `TypedEntryEmitter::Cpp` arm called
  `emit_cpp::emit_typed_monitored(&plan, &monitor_rows, &age_rows)`;
* the `TypedEntryEmitter::C` arm called `emit_c::emit_typed(&plan)` — the rows
  were read from the model and then not passed;
* `emit_c.rs` had no monitor view and `packs/entry/c/entry.c.jinja` no install
  block.

So a pure-C image whose bringup declares contracts gets no `/diagnostics`
violation reports, and nothing — no warning, no refusal — says the declaration
was dropped. RFC-0052 forbids exactly this: a declared monitor that silently
does not run. Issue 0671 was the same failure for the age half of the Rust
road.

## Why it is cheap now

The install is C ABI already: `nros_cpp_monitor_row_t`, `nros_cpp_age_row_t`,
`nros_cpp_monitor_tables_t` and `nros_cpp_install_monitors` are declared in
`nros/nros_cpp_ffi.h`, which every C entry includes.

Since phase-474 the C pack renders `nros_entry_lower::LoweredEntry` directly,
and the lowering ALREADY carries the sliced tables for every pack
(`LoweredEntry::monitors` for the single executor, `TierSetup::monitors` per
tier, `monitor_tables` for the file-scope statics) — `run_entry` now passes the
rows to every pack. The fix is therefore a TEMPLATE change in
`packs/entry/c/`: the statics block and an install block per setup, the C
spelling of `packs/entry/cpp/monitor_install.jinja`. No Rust.

It was deliberately not made in phase-474, whose acceptance is that every
existing entry renders byte-identically; this is an output change for every C
image with contracts, and wants its own golden row (a C plan with monitor rows
— none exists) and a built C fixture that declares a contract.

## Acceptance

* A golden row: a C plan with monitor and age rows, single-executor and tiered.
* A C fixture with a declared contract builds, and a violation reaches
  `/diagnostics` (the `contract_monitor_parity` shape, on the C road).
