---
id: 2053
title: "diagnostic_updater has no DiagnosticTask hierarchy: FrequencyStatus, TimeStampStatus and Heartbeat are absent"
status: resolved
type: enhancement
area: [api, cpp]
severity: low
found: 2026-10-25
resolved: 2026-10-25
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

## Resolution

`packages/api/nros-diagnostic-updater/` now has upstream's task hierarchy,
checked against the ROS 2 Humble headers installed on the host:

- `diagnostic_updater.hpp`: `DiagnosticTask` with a virtual
  `run(DiagnosticStatusWrapper&)`, `GenericFunctionDiagnosticTask<T>`,
  `FunctionDiagnosticTask`, `CompositeDiagnosticTask`, and
  `Updater::add(DiagnosticTask&)`.
- `update_functions.hpp`: `FrequencyStatusParam`, `FrequencyStatus`,
  `TimeStampStatusParam`, `TimeStampStatus` and `Heartbeat`, with upstream's
  levels, messages and key names. `FrequencyStatus` sizes its window in the
  constructor, so `tick()` allocates nothing.
- `publisher.hpp`: `HeaderlessTopicDiagnostic` and `TopicDiagnostic`.

`DiagnosticStatusWrapper::mergeSummary` now follows upstream's rule.
`CompositeDiagnosticTask` depends on it. Before, it joined every message.
Now it joins messages only when both levels are above OK; otherwise the worse
status's message wins. It also gained the overload that takes a
`DiagnosticStatusWrapper`.

`packages/api/nros-cpp/tests/compile/diagnostic_tasks_runtime.cpp` runs in
`just check cpp`, on the stub backend:

- A 50 Hz timer ticks three `FrequencyStatus` tasks. The test asserts OK for
  the task whose band contains the rate, and WARN for the tasks whose bands
  are below it and above it.
- An untouched `FrequencyStatus` reports ERROR.
- A `TimeStampStatus` reports OK for a current stamp and ERROR for a stamp
  ten seconds old.
- A composite reports its worst child.
- `Updater::add(DiagnosticTask&)` runs the task on a publish.

Two negative controls were measured:

- Disabling the too-low branch fails the below-band case.
- Making `add(DiagnosticTask&)` a no-op fails the updater case.

## Deliberate differences

- **No lock.** Upstream guards each task with a `std::mutex`. `<mutex>` is
  not available on every target this header compiles for (the FreeRTOS
  `rclcpp-compat-smoke` image is one), and nano-ros's executor runs a node's
  callbacks on one thread. A node that calls `tick()` from a thread of its
  own must keep that call from overlapping an update.
- **The clock argument is a `const rclcpp::Clock*`** where upstream takes
  `rclcpp::Clock::SharedPtr`, because `node->get_clock()` returns a pointer
  here. The `node->get_clock()` spelling still works.
- **`DiagnosedPublisher` is absent.** It stores a copy of the publisher's
  `SharedPtr`, and nano-ros's `Publisher<M>::SharedPtr` is an `rclcpp::Owned`
  handle, which can be moved but not copied. `publisher.hpp` and the porting
  guide give the replacement: keep the publisher and a `TopicDiagnostic`, and
  call `tick(stamp)` next to `publish(msg)`.
