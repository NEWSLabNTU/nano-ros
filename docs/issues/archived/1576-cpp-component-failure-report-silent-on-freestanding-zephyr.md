---
id: 1576
title: "`report_component_failure` prints nothing on a freestanding Zephyr C++ image — the entry's `FAILED at …` line never appears"
status: resolved
type: bug
area: [cpp, zephyr, logging]
severity: low
found: 2026-09-29
related: [issue-1551, issue-1425, issue-0589]
---

## What happens

When a component's setup fails, the generated C++ entry reports it through
`nros::detail::report_component_failure` (`packages/api/nros-cpp/include/nros/node.hpp:324`),
which goes through `NROS_ERROR`. `log.hpp` defines `NROS_LOG_SINK` as one of
two arms:

- an `fprintf` arm, which needs `__STDC_HOSTED__`;
- the fallback `((void)(level), (void)(file), (void)(line))`.

A freestanding Zephyr C++ image takes the fallback, so the component-level
`FAILED at …` line **compiles to nothing**. This was observed in issue 1551's
defect-2 run (derived-tiers-cpp, native_sim, heap forced small). The
publisher create returned `BAD_ALLOC` and the entry shut down correctly, but
the only diagnostic was the RMW adapter's own `nros_log` line. Nothing named
the component or the call that failed.

## Direction

Route the default sink through `nros_log` (the C ABI sink that
`ensure_default_sinks()` installs since #1432), not through `fprintf`. That
reaches `LOG_ERR`/`printk` on Zephyr and every other `no_std` target.
`NROS_LOG_SINK` stays user-overridable. The `((void)…)` arm should then be
the fallback only when no nros library is linked at all.

## Acceptance

- The same forced-OOM image as 1551 defect 2 prints the component and the
  failing call.
- A hosted build's output is unchanged.

## Resolution

`NROS_LOG_SINK`'s freestanding default (`packages/api/nros-cpp/include/nros/log.hpp`)
is now the `nros_log` C ABI: `nros_log_emit_fmt_at(nros_log_default_logger(), …)`,
with the family's level literal mapped to `NROS_LOG_SEVERITY_*` and the caller's
`file`/`line` carried through. Every nros-cpp image links that ABI (the staticlib
bundles nros-c, and the `RCLCPP_*` family already calls it), so the sink
`ensure_default_sinks()` installs reaches `LOG_ERR`/`printk` on Zephyr and the
platform console elsewhere. `report_component_failure`, and every other
`NROS_ERROR` in the headers (`nros.hpp`, `node.hpp`'s depth/param contract
diagnostics), now emit on a freestanding image.

- **Hosted output is unchanged** — the `fprintf(stderr)` arm is first and
  untouched.
- **`NROS_LOG_SINK` stays overridable** — the whole block is still under
  `#ifndef NROS_LOG_SINK`.
- **The `((void)…)` arm is opt-in**: `-DNROS_LOG_SINK_DISCARD` for a TU that
  links no nros library at all.

### Measured

Freestanding compile of a TU calling
`nros::detail::report_component_failure("talker", "create_publisher", -7)`
with the pinned `arm-none-eabi-g++` 13.2 (`-mcpu=cortex-m3 -mthumb -std=c++14
-ffreestanding -fno-exceptions -fno-rtti -O1`, the NuttX config snapshots
forwarded as the per-build header, the same trick
`check-cpp-hosted-minimal-libcpp.sh` uses):

| header | undefined symbols in the object |
| --- | --- |
| `origin/main` `log.hpp` | *(none — the call compiled to nothing)* |
| this fix | `nros_log_default_logger`, `nros_log_emit_at`, `vsnprintf` |

and the disassembly carries `bl nros_log_emit_fmt_at` → `bl nros_log_emit_at`.
With `-DNROS_LOG_SINK_DISCARD` the object again references nothing; on a hosted
`g++` the object references `fprintf`/`fputc`/`stderr` exactly as before.

**Not measured:** booting 1551's forced-OOM derived-tiers-cpp native_sim image
and reading the line off its console — that image has no lane (issue 1575). The
compile evidence shows the call is present where it was absent; that the
default sink prints it on Zephyr rests on #1432's `ensure_default_sinks()`,
which is what already carries the RMW adapter's line in that same run.
