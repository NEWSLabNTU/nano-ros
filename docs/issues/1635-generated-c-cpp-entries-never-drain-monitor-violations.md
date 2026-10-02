---
id: 1635
title: "No generated C or C++ entry drains the contract monitor's violation ring,
  so a contract violation on those roads is a log line and never reaches
  `/diagnostics`"
status: open
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
