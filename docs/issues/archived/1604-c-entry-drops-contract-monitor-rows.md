---
id: 1604
title: "A C entry drops the model's contract monitor rows: the C pack renders no
  `nros_cpp_install_monitors` block, so a C image with declared contracts runs
  unmonitored and says nothing"
status: resolved
type: bug
area: codegen, cli
severity: medium
related: [phase-462, phase-474, rfc-0052, rfc-0091, issue-1635]
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

## Resolution (2026-10-02)

Template-only, as predicted: `packs/entry/c/` gains the C spelling of
`packs/entry/cpp/monitor_install.jinja`. No Rust changed in the lowering or
the renderer — `LoweredEntry` already carried `monitors`, `monitor_tables`
and `TierSetup::monitors`.

* `entry.c.jinja` renders, per non-empty table, file-scope
  `nros_cpp_monitor_row_t` / `nros_cpp_age_row_t` arrays and their storage,
  and installs the table through `nros_cpp_install_monitors` in BOTH bodies —
  the single `__nros_entry_setup(executor)` (after the sched block, before the
  first node) and each `__nros_entry_setup_tier_N(executor)` (that tier's
  slice, before its first node). A refused install (over
  `NROS_EXECUTOR_MAX_MONITORS`) returns its code and aborts setup, so it can
  never run unmonitored silently.
* The storage is `static uint64_t …[(n * NROS_CPP_*_ROW_STORAGE + 7u) / 8u]`:
  8-aligned without C11 `_Alignas` (the C pack is C99) and sized from the ABI's
  own per-row constant. Row initialisers are positional, exactly as the C++
  pack's.
* New partial `packs/entry/c/monitor_install.jinja`, registered in
  `pack.toml` (`check-entry-pack-conformance` reads it back).
* An image with no rows renders byte-identically: every pre-existing golden
  is unchanged.

### Measured

Goldens (`codegen::entry::golden`, new `CMonitored` / `CppMonitored`
emitters with one shared row fixture): `c_native_monitors.c.golden`
(single executor: a rate row and an age row, plus a fixture row for a node
the image does not construct, which the lowering drops),
`c_native_tiers_monitors.c.golden` (tier 0 gets `/ctrl/cmd`, tier 1
`/telem/telemetry` + an age row; each tier installs only its own) and
`cpp_native_monitors.cpp.golden` beside them as the reference — no C++
golden recorded the monitor region before either. Every line read.
`tests_c.rs` gains the three monitor tests `tests_cpp.rs` has (row parity
with `render_monitor_rs`, install-before-create, zero-cost byte identity,
per-tier slicing). Reverting `entry.c.jinja` alone fails all four (3 tests +
the golden harness).

On a REAL pure-C image — `examples/workspaces/c` copied to an untracked
scratch dir, with a `launch/system.contract.yaml` declaring
`talker.pub.chatter.min_rate_hz` (the talker publishes at 1 Hz), built with
this tree's CLI (`nros sync && nros build native`), run against a private
`rmw_zenohd` for 14 s:

| contract | generated `native_entry_nros_main_generated.c` | `nm native_entry` | runtime |
| --- | --- | --- | --- |
| `min_rate_hz: 5`, BEFORE (C template reverted, CLI rebuilt) | no monitor region, no install call | no `__nros_mon_*` symbol (`nros_cpp_install_monitors` is present only as a library export) | no `contract violation` line over 13 deliveries — the contract silently unmonitored |
| `min_rate_hz: 5`, after | `{ "/chatter", "/talker/chatter", 5000u, 0u }` + install | `d __nros_mon_rows`, `b __nros_mon_storage`, `T nros_cpp_install_monitors` | `[WARN] nros: contract violation: rate-hierarchy-runtime /talker/chatter measured=999 declared=5000` |
| `min_rate_hz: 0.5`, after (compliant control) | `… 500u, 0u` | same symbols | no `rate-hierarchy-runtime` line over 13 deliveries |

`measured=999` (milli-Hz) is the 1 Hz publish rate — the C component's
`nros_cpp_publish_raw` bumps the row's cell, so the monitor measures rather
than reporting a dead zero.

### Not measured

* The report is the runtime's log floor (`monitor::log_violation`). A C image
  has no `/diagnostics` reporter: `nros_cpp_executor_drain_violations` exists
  but neither the C nor the C++ generated entry drains into a
  `DiagnosticArray` — the C++ road's parity test prints the drain itself
  (`contract-monitor-cpp`). So "a violation reaches `/diagnostics`" from the
  acceptance list is met on neither road by the generated entry; what both
  roads do is install and log. It is the same gap on both
  roads; filed as issue 1635.
* The age half on a real image: `std_msgs/Int32` carries no stamp, so no age
  row was exercised at runtime (golden + unit test only).
* No RTOS C image was built; the template is board-independent (the tiered
  golden covers the per-tier shape on native).
* No `fixtures.toml` row was added for a contracted C image — the proof
  workspace is a scratch copy, not a fixture.
