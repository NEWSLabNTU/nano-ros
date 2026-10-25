---
id: 2053
title: "diagnostic_updater has no DiagnosticTask hierarchy: FrequencyStatus, TimeStampStatus and Heartbeat are absent"
status: open
type: enhancement
area: [api, cpp]
severity: low
found: 2026-10-25
related: [phase-488]
---

## What

`packages/api/nros-diagnostic-updater/` gives `diagnostic_updater::Updater`
plain callbacks (`add(name, TaskCallback)` and `add(name, obj, &T::method)`).
Upstream's `<diagnostic_updater/update_functions.hpp>` also declares
statistics tasks, each a `DiagnosticTask` subclass that an `Updater` adds as an
object:

- `FrequencyStatus` with `FrequencyStatusParam`, which counts `tick()` calls
  against a window and reports a rate outside a min/max band;
- `TimeStampStatus` with `TimeStampStatusParam`, which reports stamps too far
  behind or ahead of now;
- `Heartbeat`, which always reports OK.

Upstream's `diagnostic_updater.hpp` also declares `DiagnosticTask`,
`CompositeDiagnosticTask` and `FunctionDiagnosticTask`, and its
`publisher.hpp` declares `HeaderlessTopicDiagnostic`, `TopicDiagnostic` and
`DiagnosedPublisher`, which are built from these classes.

Phase-488 W3 added `update_functions.hpp` with no classes, because Autoware's
`autoware_external_cmd_selector` @0.45.0 includes it and uses only `Updater`.
Code that names one of these classes fails to compile on the class's name.

## Who uses them

Measured 2026-10-25 with `grep -rlE 'FrequencyStatus|TimeStampStatus|DiagnosedPublisher|TopicDiagnostic'`
over the C++ sources of an Autoware 0.45.1 workspace (`src/`, every repository
`autoware.repos` checks out): **no file uses them**, while 127 files mention
`diagnostic_updater`. Upstream `autoware_universe` `main` (surveyed the same
day) has none either. So this gap has not been seen in a port yet. It is filed
because a ported driver or monitor outside Autoware is the likely first user:
`TopicDiagnostic` is how such nodes report a topic's rate.

## Done when

`DiagnosticTask` exists with a virtual `run(DiagnosticStatusWrapper&)`, and
`Updater::add(DiagnosticTask&)` exists. `FrequencyStatus`, `TimeStampStatus`
and `Heartbeat` exist in `update_functions.hpp`, without heap allocation per
`tick()`. A test in `just check cpp` runs a `FrequencyStatus` against a timer
on the stub backend and asserts the level it reports for a rate inside the
band and for one outside it.
