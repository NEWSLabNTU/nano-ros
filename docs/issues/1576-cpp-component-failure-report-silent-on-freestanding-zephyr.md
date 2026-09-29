---
id: 1576
title: "`report_component_failure` prints nothing on a freestanding Zephyr C++ image — the entry's `FAILED at …` line never appears"
status: open
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
