# Trace callbacks

<!-- type: how-to · level: 3 -->

**Needs first:** [Pick a scheduling model](pick-a-scheduling-model.md)

Trace callbacks to see where each one starts and ends on the board's own
clock, and mark your application's phases on the same clock. The executor
emits one `(marker_id, arg)` event per callback start, end and registration
through a trace sink: a function you install, or one the platform installs
for you. Your application emits its own events through the same sink.

## Before you start

- An image built with callback tracing: the `trace-callbacks` feature of
  `nros-c`, `nros-cpp` or `nros`. Without it every call below is a
  no-op, and the marker call still checks its id.
- A sink. Zephyr installs one when CTF tracing is on (see the table below).
  Elsewhere, install your own before the executor registers anything.

## Steps

1. Install a sink, if the platform does not. The signature is two `uint32_t`
   values, the shape of a platform trace event, so a platform shim installs
   as is. It runs on every thread that dispatches callbacks, so it must be
   re-entrant:

   ```c
   #include <rclc/rclc.h>

   static void my_sink(uint32_t marker_id, uint32_t arg) {
       /* write (marker_id, arg) to a ring, a UART, a trace stream */
   }

   rclc_set_trace_sink(my_sink);
   ```

2. Mark your application's own events with an id from the application block,
   1 to 15:

   ```c
   enum { PHASE_SENSE = 1, PHASE_PLAN = 2, PHASE_ACT = 3 };

   rclc_trace_marker(PHASE_PLAN, cycle);
   ```

   ```cpp
   #include <rclcpp/rclcpp.hpp>

   rclcpp::trace_marker(PHASE_PLAN, cycle);
   ```

   ```rust
   use nros::trace_marker::emit_app_marker;

   emit_app_marker(PHASE_PLAN, cycle)?;
   ```

   The event goes through the installed sink on the same path as the
   executor's events, so it is ordered with them. With no sink installed the
   call costs a range compare, one load and one branch.

## Marker ids

| ids | owner |
| --- | --- |
| 0 | reserved, never emitted |
| 1 to 15 | your application (`RCLC_TRACE_APP_MARKER_FIRST` to `RCLC_TRACE_APP_MARKER_LAST`; C++ `rclcpp::kTraceAppMarkerFirst` to `kTraceAppMarkerLast`) |
| 16 to 27 | the executor: callback registration, name, start, end, contract violations, takes |
| 28 and up | reserved for the executor's future events |

An id outside 1 to 15 is refused: C returns `RCL_RET_INVALID_ARGUMENT` and
sets the error that `rcl_get_error_string` reads, C++ returns
`ErrorCode::InvalidArgument`, Rust returns `AppMarkerIdOutOfRange`. Each
logs an error and emits nothing. The id is never moved onto another one.

## Where the events go

| Platform | Default sink |
| --- | --- |
| Zephyr | `sys_trace_app_marker`, into the CTF stream as an `app_marker` event (fields `marker_id`, `arg`), when `CONFIG_TRACING_CTF` is on. `nros setup board` adds that event to the Zephyr workspace, so no patch of your own is needed. The sched-task entry installs it, and keeps a sink you installed first. |
| FreeRTOS | none. Install a sink of your own, for example a RAM ring you read with a debugger. |
| POSIX (Linux) | none. Install a sink of your own. |
| NuttX, ThreadX | none. Install a sink of your own. |

### A Zephyr tracing backend of your own

Zephyr writes the CTF stream through one backend. To keep it in a recorder of
your own, such as an overwrite-oldest RAM ring, register the backend in your
application and select it by name. `nros setup board` adds the two Kconfig
symbols that select it, on Zephyr 3.7 and 4.4; stock Zephyr has no way to
select an out-of-tree backend.

```c
#include <tracing_backend.h>

static void ring_init(void) { /* ... */ }
static void ring_output(const struct tracing_backend *backend,
                        uint8_t *data, uint32_t length) { /* ... */ }

static const struct tracing_backend_api ring_api = {
        .init = ring_init,
        .output = ring_output,
};
TRACING_BACKEND_DEFINE(my_trace_ring, ring_api);
```

```text
CONFIG_TRACING=y
CONFIG_TRACING_CTF=y
CONFIG_TRACING_SYNC=y
CONFIG_TRACING_BACKEND_CUSTOM=y
CONFIG_TRACING_BACKEND_CUSTOM_NAME="my_trace_ring"
```

The name is the first argument of `TRACING_BACKEND_DEFINE()`. No in-tree
backend is built.

## If it doesn't work

- **No events at all.** The image was built without `trace-callbacks`, or no
  sink is installed. `rclc_trace_marker` returns `RCL_RET_OK` in both cases.
- **Callbacks show as `handle N` with no names.** The sink was installed after
  the executor registered its callbacks; install it first.
- **`rclc_trace_marker` returns `RCL_RET_INVALID_ARGUMENT`.** The id is
  outside 1 to 15. Ids from 16 up belong to the executor.
- **Zephyr: `CONFIG_TRACING_BACKEND_CUSTOM` is an undefined symbol.** The
  workspace was provisioned before nano-ros carried the patch. Re-run
  `nros setup board <board>`.

## See also

- [Analyze timing](analyze-timing.md): the static checks for a callback's
  worst case.
- [C API](../../reference/c-api.md) and [C++ API](../../reference/cpp-api.md).
