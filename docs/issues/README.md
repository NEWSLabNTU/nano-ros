# nano-ros Issues

This directory tracks nano-ros bugs, limitations, and tech-debt — one file
per issue, mirroring the repo's numbered-RFC convention
(`docs/design/NNNN-*.md`) and the roadmap `archived/` pattern. Each file
carries YAML frontmatter plus the issue body (problem, evidence, current
state, fix/direction). Open issues live directly in `docs/issues/`; resolved
ones move to `docs/issues/archived/`.

## Conventions

**Frontmatter schema** (every issue file):

```yaml
---
id: 7                    # the issue number (matches the 4-digit filename id)
title: Unbounded message sequences capped at 64 elements
status: open             # open | resolved | wontfix
type: enhancement        # bug | enhancement | tech-debt
area: codegen            # codegen | rmw | memory | cmake | zephyr | nuttx | freertos | threadx | build | testing
related: []              # e.g. [rfc-0023, phase-218] — cross-links to RFCs / phases
resolved_in:             # (resolved only) commit short-hash or phase, e.g. "Phase 140"
---
```

**Lifecycle**:

1. Open an issue as `docs/issues/NNNN-slug.md` with `status: open`.
2. When resolved, set `status: resolved` + `resolved_in:` and **move** the
   file to `docs/issues/archived/NNNN-slug.md` (trimmed to a terse
   resolution summary).
3. **Numbering** = the next integer after the highest existing id.
   **Slug** = a kebab-case form of the title; the filename id is the
   zero-padded 4-digit issue number.

## Issue vs RFC vs phase doc

- **Issue** (`docs/issues/`) = a bug, limitation, or tech-debt item.
- **RFC** (`docs/design/NNNN-*.md`) = a design decision.
- **Roadmap phase** (`docs/roadmap/`) = an implementation plan.

Issues cross-link to the RFCs and phases that inform or resolve them via the
`related:` frontmatter field.

## Open issues

| id | title                                                                 | type        | area   | file |
|----|-----------------------------------------------------------------------|-------------|--------|------|
| 35 | zephyr native_sim e2e fail consistently (XRCE-heavy) — not load flakes | bug        | zephyr | [0035-zephyr-native-sim-e2e-consistent-failures.md](0035-zephyr-native-sim-e2e-consistent-failures.md) |
| 41 | suite-wide compile-in-tests antipattern — convert to build-stage fixtures | tech-debt | testing | [0041-compile-in-tests-suite-wide.md](0041-compile-in-tests-suite-wide.md) |
| 44 | esp-idf platform.c build fails — `_heap_start`/`_heap_end` undeclared | bug | esp32 | [0044-esp-idf-platform-c-heap-symbols-undeclared.md](0044-esp-idf-platform-c-heap-symbols-undeclared.md) |
| 42 | platform/std-header architecture fragile — recurring libc/std clashes (#27/#36/#38) | tech-debt | c-api | [0042-platform-header-architecture-fragility-libc-std-clashes.md](0042-platform-header-architecture-fragility-libc-std-clashes.md) |

Resolved issues live in [`archived/`](archived/). Recently resolved (Phase 239):
**#39** — C++ `init_with_launch_auto` null-locator env-fallback (fixed in the
3-arg `init` overload); **#40** — C++ action callback truncated result (a symptom
of #39 + a latent result offset 8→5); **#43** — C++ action server empty result
for a C-framed goal (a stale pre-233.6 C fixture writing a removed GoalId
sequence prefix; resolved by a fresh build); **#45** — FreeRTOS Entry-pkg
build/panic-handler (Component → rlib-only + board-owned `panic_semihosting` +
`mps2_an385.ld`); **#46** — FreeRTOS Entry-pkg stack-overflow at Executor
(app-task stack 256→384 KiB + zenoh heap 512 KiB→2 MiB; runtime gate un-ignored +
green); **#48** — FreeRTOS Entry firmware never connected over zenoh: the zenoh
RMW backend was never linked/registered (→ `NoBackend`) and the deploy
locator/ip/gateway was inert (`Config::default()` `192.0.3.x`). Fixed by linking
+ registering the backend (`nros/rmw-zenoh` + `__register_linked_rmw()` on
`target_os = "none"`) and threading the deploy block into the boot `Config` via
`BoardEntry::run_with_deploy` + `DeployOverlay`; `freertos_run_plan_runtime` now
asserts the connected run. See `archived/0039-*`, `archived/0040-*`,
`archived/0043-*`, `archived/0045-*`, `archived/0046-*`, `archived/0048-*`.

Recently resolved (Phase 240.5): **#47** — C/C++ action client now callback-based
(`nros::bind_action_client` = `set_callbacks` + a poll-timer pump per RFC-0041);
NuttX cpp+C action E2E green in QEMU. See `archived/0047-*`.
