---
id: 2065
title: "The Zephyr level worker frame is an unmeasured 4,096 B estimate"
status: open
type: tech-debt
area: [zephyr, executor, memory]
severity: low
found: 2026-10-26
related: [phase-509, rfc-0108, 2019]
---

## Measured

Issue 2019 measured a spawned level's worker frame on FreeRTOS and NuttX, and
both came out well above the old 4,096 B estimate: the level's setup runs on
its own stack and is the stack's peak.

| Port | C runner | Rust runner |
|------|---------:|------------:|
| FreeRTOS, mps2-an385 | 10,240 B | 48,128 B |
| NuttX, virt cortex-a7 | 11,264 B | 32,768 B |
| Zephyr | 4,096 B (estimate) | 4,096 B (estimate) |

Zephyr still uses the estimate, `CONFIG_NROS_ZEPHYR_LEVEL_WORKER_FRAME`,
for both runners. No board has measured it. A Zephyr image whose level stacks
derive from member `stack_bytes` adds this frame to the members' sum. If the
real frame is two or more times the estimate, as it is on the other two
ports, the derived stack is too small and nothing reports it.

The one Zephyr level image in use, the simple-autoware-safety-island board
(S32K344, C++ runner, three levels), sets its level stacks by hand to
16,384 B. The lock-probe image read a 12.3-12.5 KB peak on them, but that
peak is the whole level, not the worker frame alone.

## Closes when

- A Zephyr level image measures the worker frame for its runner (setup peak
  and steady peak, with `CONFIG_THREAD_ANALYZER` or the stack sentinel), and
  the Kconfig default becomes the measured value, or a stated bound on it.
- The Rust runner's Zephyr frame is measured too, or the Kconfig help says
  which runner the default covers.
- `nros-mem-report.py`'s Zephyr default agrees with the Kconfig.
