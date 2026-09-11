---
id: 1303
title: "The two RUNTIME refusals emit through the legacy `NROS_ERROR` sink, which
  is a no-op on every freestanding target — so both abort anonymously on an RTOS"
status: open
type: bug
area: [api]
related: [1019, 1302, phase-417, rfc-0089]
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
