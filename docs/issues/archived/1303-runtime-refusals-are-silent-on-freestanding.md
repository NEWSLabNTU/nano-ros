---
id: 1303
title: "The two RUNTIME refusals emit through the legacy `NROS_ERROR` sink, which
  is a no-op on every freestanding target — so both abort anonymously on an RTOS"
status: resolved
type: bug
area: [api]
related: [phase-482, 1019, 1302, 1576, phase-417, rfc-0089]
resolved: 2026-10-10
---

## What

RFC-0089 §"Where the refusal fires" puts a refusal at RUNTIME when only the VALUE
carries the defect. Two such sites exist in `packages/api/nros-cpp/include/nros/nros.hpp`,
and **both die without saying anything on the targets nano-ros exists for**:

| site | message | emitted with |
| --- | --- | --- |
| `rclcpp::init(argc, argv)` when `--ros-args` is present | `NROS_RCLCPP_REFUSE_INIT_ARGV` | `NROS_ERROR("%s", …)` then `::std::abort()` |
| `rclcpp::detail::abort_failed_create` | `NROS_RCLCPP_ABORT_FAILED_CREATE` | `NROS_ERROR("rclcpp::Node::%s(…) …", …)` then `::std::abort()` |

`NROS_ERROR` is the legacy printf family. Its sink is `fprintf(stderr)` on a
hosted build and, on a freestanding one,
`((void)(level), (void)(file), (void)(line))` — a **no-op**
(`packages/api/nros-cpp/include/nros/log.hpp`). Both headers are parsed
freestanding (`just check cpp`'s header sweep compiles `nros.hpp` with
`-ffreestanding`), so on Zephyr, FreeRTOS, NuttX and ThreadX a ported node that
passes `--ros-args`, or whose `create_publisher` fails, **aborts with no
diagnostic at all**.

This is issue 1019's first defect, in the one place where it is worst: 1019 was
about log lines going missing, and this is about the message that explains why
the process just died.

## Why it was not fixed with 1019

Issue 1019's fix re-pointed the `RCLCPP_*` macro family at `NROS_LOG_*`, which
does reach `nros_log` on every target. These two sites are not macros in that
family and were left alone deliberately, for one measured reason:
`failed_create_aborts.cpp` — the probe that holds `abort_failed_create`'s
behaviour — links **no nano-ros archive** and passes
`-Wl,--unresolved-symbols=ignore-all` for the issue-0360 variant anchors. That
works only because the anchors are DATA. Give the header a call to
`nros_log_emit_at` and the probe's binary stops loading entirely
(`unexpected PLT reloc type 0x00`), which was measured while writing
`publisher_publish_guards_initialized.cpp` in the same phase. So the fix has to
move the probe's link model, not just the header.

## The second half: the message has to FIT

`packages/api/nros-cpp/include/nros/log.hpp` grew
`rclcpp::detail::say_refused` for exactly this, and it carries the other half of
the finding, measured: `nros_log`'s format buffer is 256 bytes
(`nros_log::format_buffer_capacity`) and `heapless::String::push_str` is
**all-or-nothing**, so a body that does not fit is DROPPED rather than truncated
— a 1050-byte refusal reached the console as `[ERROR] nros: [ts] …` and nothing
else. `rclcpp::detail::RUNTIME_REFUSAL_MAX` (160) is a `static_assert` on the
emitter so a new runtime refusal cannot vanish that way.

`NROS_RCLCPP_REFUSE_INIT_ARGV` is ~620 bytes and
`NROS_RCLCPP_ABORT_FAILED_CREATE` is ~900. Both therefore need a SHORT runtime
form beside the long text, the way
`NROS_RCLCPP_REFUSE_UNBOUNDED_SPIN_RUNTIME` does — routing them at `nros_log`
without shortening them would replace a silent abort with an abort that prints an
ellipsis.

## Acceptance

* Both sites emit through `nros_log` (so the message reaches `printk`/`LOG_ERR`
  on an RTOS), with a runtime form short enough to pass
  `RUNTIME_REFUSAL_MAX`.
* `failed_create_aborts.cpp` still runs — which means deciding its link model,
  not weakening what it asserts. It checks the CHILD'S STDERR for the migration
  text, so the chosen sink has to reach stderr on the host.
* A freestanding-target check that the message is reachable at all; today's
  header sweep is `-fsyntax-only` and is green either way.

## Resolution (2026-10-10, phase-482 W7)

**Half of the premise had already gone when this was fixed.** Issue 1576
(`099ad2adb5`) made the freestanding default of `NROS_LOG_SINK`, and with it
`NROS_ERROR`, emit through `nros_log`. Measured: an object compiled
`-ffreestanding` from the pre-fix headers already calls `nros_log_emit_at`
for both refusals. The route was fine. The LENGTH was not: as one record the
`--ros-args` refusal was over 870 bytes and the failed-create one over 1180,
`nros_log_emit_fmt_at` cut each to 255, and a real sink drops a body that long
(the second half of this issue). So both still reached an RTOS console as a
header and an ellipsis.

The fix, in `nros-cpp`:

- Each site emits TWO records through `nros_log`. The first carries the
  variable part with an explicit bound: `rclcpp::Node::%s("%.64s") failed:
  rclcpp::ErrorCode %d` and `rclcpp::init: refused: %.120s`. The second is the
  fixed text, said through `NROS_RCLCPP_SAY_REFUSED`, whose `static_assert`
  holds it to `RUNTIME_REFUSAL_MAX`.
- `NROS_RCLCPP_REFUSE_INIT_ARGV` and `NROS_RCLCPP_ABORT_FAILED_CREATE` are now
  those short texts (158 and 149 bytes). The long explanations they used to
  carry are already where a reader looks them up: RFC-0089, the `cpp:init` and
  `cpp:Node::create_publisher` ledger rows, and the doc comment on
  `require_created`.

The test program's link model (the maintainer decision this needed) stays
self-contained. `tests/compile/nros_log_stderr_sink.hpp` defines the two
`nros_log` entry points the header reaches, writing to stderr, and writes a
marker in place of any record longer than `RUNTIME_REFUSAL_MAX`.
`failed_create_aborts.cpp` asserts the marker is absent, with a 200-byte topic
name among its cases. Mutation-checked: an unbounded `%s` for the name fails it
with "every record of the refusal must fit rclcpp::detail::RUNTIME_REFUSAL_MAX".
`init_honours_ros_args.cpp` uses the same sink.

The freestanding check the acceptance asked for is
`tests/compile/refusal_reaches_nros_log.cpp`, run by `just check cpp`: one
`-c -ffreestanding -nostdinc++` object per refusal, which must call
`nros_log_emit_at`. It guards the ROUTE, so it passes on the pre-fix headers
too. That is stated in the file rather than presented as the fix.
