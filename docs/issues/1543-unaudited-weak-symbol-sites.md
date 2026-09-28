---
id: 1543
title: "Three weak-symbol sites no audit list covers, because the gate reads one spelling under `packages/**` only"
status: open
type: bug
area: [build, platform]
severity: low
found: 2026-09-28
related: [phase-472, ]
---

## What happens

VERIFIED present on `main`, none in `check-weak-symbols.sh`'s audited allowlist:

- `packages/boards/nros-board-freertos/c/freertos_c_entry.c:155` —
  `__attribute__((weak, used)) void zpico_set_task_config(…)`
- `zephyr/heap_stub_native.c`
- `zephyr/cyclonedds-zephyr/link_stubs.c`

The gate reads `git ls-files 'packages/**'` and matches the exact spellings
`__attribute__((weak))` and `.weak `. So `(weak, used)` is unmatched, and `zephyr/`
is unread. `__weak`, `#pragma weak` and `[[gnu::weak]]` are unmatched too.

## Why it matters

A weak symbol is a silent link-time override: the build succeeds whichever
definition wins. The allowlist exists so each one is a decision someone made.

## Fix

Audit the three sites; widen the gate to all owned C/asm and match
`__attribute__\s*\(\([^)]*\bweak\b`, `__weak` and `#pragma weak` (phase-472 W5).
