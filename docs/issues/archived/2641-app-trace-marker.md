---
id: 2641
title: "No public call emits an application event through the installed trace sink"
status: resolved
type: enhancement
area: [api, executor, c-api, cpp]
severity: medium
found: 2026-11-16
resolved: 2026-11-16
resolved_in: "feat(issue 2641): an application trace marker through the installed trace sink"
related: [phase-478]
---

**Home:** phase-478 I29 (found by the safety island). GitHub #4009.

## Symptom

The island marks its control-cycle phases by calling `sys_trace_app_marker`
directly under `CONFIG_TRACING_CTF`
(`actuation_module/src/asi_common/include/common/diag/trace_marker.hpp`).
nano-ros has the sink seam (`rclc_set_trace_sink`,
`rclc_set_default_trace_sink`) and emits the executor's own callback events
(ids 16-27) through it, but offered no public call that emits an
APPLICATION event through the same sink. So the island's markers reach a
trace only on Zephyr with CTF, and on FreeRTOS (an536) it reads them with
gdb scripts.

## Fix

One call per language, all funnelling into
`nros_node::executor::trace_marker::emit_app_marker`:

- C: `rcl_ret_t rclc_trace_marker(uint32_t id, uint32_t arg)`, with
  `RCLC_TRACE_APP_MARKER_FIRST` / `RCLC_TRACE_APP_MARKER_LAST`.
- C++: `rclcpp::Result rclcpp::trace_marker(uint32_t id, uint32_t arg)`, with
  `rclcpp::kTraceAppMarkerFirst` / `kTraceAppMarkerLast`, over
  `nros_cpp_trace_marker`.
- Rust: `nros::trace_marker::emit_app_marker(id, arg) -> Result<(),
  AppMarkerIdOutOfRange>` (re-exported from `nros-node`).

The event goes to whatever sink is installed, through `callback_trace`'s own
`sink()`/`emit` path, so it shares the executor events' clock and ordering.
No per-platform path was added.

### The id range: 1..=15

`callback_trace` already started the executor's block at 16 "rather than 8 so
those keep room to grow contiguously": ids 1..7 were the island's hand-placed
phase markers, and 8..15 the room left for them. Taking that block means the
island's existing ids keep their meaning in every capture already taken, and
the executor's block (16-27, growing upward) is unreachable from an
application call. 0 is refused too: it is what a zeroed id reads as. A
`const` assert in `callback_trace` pins the block below `MARKER_REGISTER`, and
one in `nros-c` pins the header literals to the core's constants.

An out-of-range id is refused, never clamped: C returns
`RCL_RET_INVALID_ARGUMENT`, sets the `rcl_get_error_string` state and logs an
error; C++ returns `ErrorCode::InvalidArgument` and logs; Rust returns
`AppMarkerIdOutOfRange`. The check is ungated, so it runs in a build without
`trace-callbacks` too. With no sink installed the call costs the range
compare, one atomic load and one branch.

### Platform reach

| Platform | What carries the event |
| --- | --- |
| Zephyr | the default sink the sched-task entry installs, `nros_zephyr_trace_marker` -> `sys_trace_app_marker` (CTF), when the image carries that event |
| FreeRTOS | an application-installed sink. nano-ros installs no default sink on FreeRTOS; the mps2 board's tband ring is a kernel trace, not this sink |
| POSIX, NuttX, ThreadX | an application-installed sink; no default |

The issue's "the ring on FreeRTOS" is the island's own ring, installed with
`rclc_set_trace_sink`; nano-ros has no FreeRTOS default sink to reuse, and
this fix does not fake one.

## Tests

- `nros-node`: `an_application_marker_reaches_the_installed_sink` (capturing
  sink, ids and args arrive, 0 and 18 refused) and
  `the_application_block_is_below_every_executor_id` (16-27 and `u32::MAX`
  refused, in every build).
- `just check c`: `packages/api/nros-c/tests/run/trace_marker.c`, linked and
  run against a `trace-callbacks` archive (events arrive) and against the
  shipped archive (`-DNROS_TEST_TRACE_OFF`: nothing reaches the sink, ids are
  still refused).
- `just check cpp`: `packages/api/nros-cpp/tests/compile/trace_marker_runtime.cpp`,
  against a `trace-callbacks` `libnros_cpp.a`, with the sink installed through
  the C seam.

Docs: the book's new "Trace callbacks" page, and the C and C++ API reference
pages.
