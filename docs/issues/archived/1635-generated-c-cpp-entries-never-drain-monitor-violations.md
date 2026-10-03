---
id: 1635
title: "No generated C or C++ entry drains the contract monitor's violation ring,
  so a contract violation on those roads is a log line and never reaches
  `/diagnostics`"
status: resolved
type: limitation
area: [codegen, cli, diagnostics]
severity: low
found: 2026-10-02
related: [issue-1604, issue-0514, phase-462, rfc-0052]
---

## What happens

Issue 1604 made the C entry pack install the model's contract monitor rows,
as the C++ pack already did (phase-462 W1). Measured on a real pure-C native
image with `min_rate_hz: 5` on a 1 Hz talker, the violation is reported as

```
[WARN] nros: contract violation: rate-hierarchy-runtime /talker/chatter measured=999 declared=5000
```

— which is `monitor::log_violation`, issue 0514's log FLOOR. Nothing in a
generated C or C++ entry drains the ring into a `DiagnosticArray`:

```
git grep -n 'drain_violations' -- packages/cli packages/boards cmake   # no hits
```

`nros_cpp_executor_drain_violations` exists (its header comment says "The
entry glue hands each one to the reporter it links"), but no entry glue calls
it. The C++ road's parity test (`contract-monitor-cpp`) drains in its OWN
`main` and prints `DIAG …` lines, so phase-462 W1's acceptance was met by the
test binary, not by a generated image. Issue 1604's acceptance ("a violation
reaches `/diagnostics`") is therefore met on neither the C nor the C++ road
by what codegen emits.

## Why it was left

Issue 0514 chose log-at-detection as the floor deliberately (it works on a
bare RTOS image, needs no publisher), and said `/diagnostics` publication
"remains open". `nros-diagnostics` has no C/C++ surface, so publishing a
`DiagnosticArray` from a C/C++ entry needs either that surface or a drain
hook in the board runner that the Rust side already links.

## What a fix needs

* A drain point the generated entry (or the board runner both packs call)
  owns, run between spins — `nros_cpp_executor_drain_violations` is the C ABI
  for it.
* A reporter reachable from C/C++ that publishes the drained verdicts on
  `/diagnostics` with the RFC-0050 rule vocabulary.
* Acceptance: a contracted C image and a contracted C++ image, each observed
  by a `/diagnostics` subscriber (the `contract-monitor-diagsink` shape).

## Resolution

Fixed 2026-10-03 on `fix/1635-drain-monitor-violations`, for the C and C++
roads. The Rust road has a larger gap underneath this one and is filed as
[issue 1676](../1676-rust-entry-never-installs-contract-monitors.md).

**Where the drain lives.** Not in the templates: the spin loop belongs to each
board's runner (native, FreeRTOS, Zephyr, ThreadX, NuttX), so an entry-side
drain would need a line per runner. The RUNTIME owns it instead:

* `nros-node`: `Executor::set_violation_sink(fn, ctx)`. Every detected
  violation goes through ONE `monitor::record_violation` — the log floor
  (issue 0514), the sink when installed, and the ring `drain_violations` reads.
  Eight sites had spelled "log if enabled; push or count the drop" by hand; the
  sink would have been a ninth copy. The ring is still filled, so a fixture or
  application that drains by hand sees what it did before. (A first version
  drained the ring into the sink at the spin tail; that emptied it under the
  C++ parity twin, which installs through `nros_cpp_install_monitors` and then
  drains itself, so it was replaced before landing.)
* `nros-diagnostics`: `DiagnosticReporter::report_violation` + `kind_for_rule`
  — the one mapping from a violation to a `DiagnosticArray` (RFC-0050 rule id
  kept verbatim, `max-age`/`silence` as assumptions, everything else a
  guarantee). The parity fixture's private copy, which folded any rule it did
  not know into `deadline-miss-runtime`, now calls it.
* `nros-cpp`: `nros_cpp_install_monitors` arms a per-executor `/diagnostics`
  publisher (`src/diag.rs`) when its table is non-empty. Both packs already
  call it, before the first node, on the single executor and on every tier's,
  so no template and no golden changed. `nros_cpp_fini` unhooks the sink and
  drops the publisher while the session is still open.
* Found on the way, and fixed with it: `CppContext` had THREE hand-written
  constructor bodies and two of its fields (`tag`, issue 0436; `in_dispatch`,
  issue 0387) had each been missed at one of them, killing every borrowed-tier
  executor. The new `diag` field would have been the third chance; all three
  call one `write_context` now.
* `nros-build-helpers`: `CPP_EXECUTOR_OPAQUE_U64S`'s overhead adds the
  reporter from the PROBED `PUBLISHER_SIZE`; `lib.rs`'s const-assert refuses an
  under-estimate at compile time.
* Lockfiles: root + the two NuttX FFI leaves gain `nros-diagnostics`
  (`just lock-update`).

**Measured** on a real pure-C native image: `examples/workspaces/c` copied to
an untracked scratch dir, `launch/system.contract.yaml` declaring
`talker.pub.chatter.min_rate_hz`, built with this tree's CLI
(`nros sync && nros build native`), run 15 s against a private `rmw_zenohd`
with `ros2 topic echo --no-daemon /diagnostics` (rmw_zenoh_cpp) observing:

| contract | image log | `/diagnostics` |
| --- | --- | --- |
| `min_rate_hz: 5` (talker at 1 Hz) | `contract violation: rate-hierarchy-runtime /talker/chatter measured=999 declared=5000` | one `DiagnosticArray`: level 2, `name: rate-hierarchy-runtime`, `hardware_id: /talker/chatter`, `message: measured 999 vs declared 5000`, `kind: guarantee` |
| `min_rate_hz: 0.5` (compliant control) | no violation, 13 deliveries | nothing |

BEFORE is issue 1604's own measurement of the same image: the log line and
nothing on `/diagnostics` (no generated entry drained the ring). Unit:
`an_installed_violation_sink_sees_every_violation_at_detection` (sink fed once
per verdict, ring kept for a hand drain, no feed after removal) and
`a_violation_report_keeps_the_rule_and_classifies_its_side`. `just check c` and
`just check cpp` green.

**Not measured:** a contracted C++ image on `/diagnostics` (it reaches the same
`nros_cpp_install_monitors` the C image does, and both packs render the same
install call — the C image is the one measured); a tiered image (each tier's
setup installs its own table, so each tier executor gets its own publisher —
read, not run); an RTOS image. The `contract_monitor_parity` fixtures were not
rebuilt and re-run: their C++ twin's hand drain is unchanged by design (the
ring still fills), and the Rust fixture now reports through the shared mapping
with the same rule ids.

Sweep: `git grep -n 'monitor_violations.push' -- packages/core/nros-node/src`
(empty: the only push is `record_violation`'s) and `git grep -n 'addr_of_mut!((\*ctx_ptr)' --
packages/api/nros-cpp/src` (field writes only inside `write_context`; the rest carve `backing`).
