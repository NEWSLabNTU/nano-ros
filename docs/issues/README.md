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
| 34 | host-integration surfaces 31 pre-existing nros-tests failures        | bug         | testing| [0034-host-integration-31-preexisting-test-failures.md](0034-host-integration-31-preexisting-test-failures.md) |
| 35 | zephyr native_sim e2e fail consistently (XRCE-heavy) — not load flakes | bug        | zephyr | [0035-zephyr-native-sim-e2e-consistent-failures.md](0035-zephyr-native-sim-e2e-consistent-failures.md) |
| 39 | C++ `init_with_launch_auto` skips the `NROS_LOCATOR`/`ROS_DOMAIN_ID` env fallback → null locator → TransportError | bug | c-api | [0039-cpp-init-with-launch-auto-skips-locator-env-fallback.md](0039-cpp-init-with-launch-auto-skips-locator-env-fallback.md) |
| 40 | C++ action-client callback path delivers a truncated result (`[0]`) and no feedback | bug | c-api | [0040-cpp-action-client-callback-result-truncated-feedback-missing.md](0040-cpp-action-client-callback-result-truncated-feedback-missing.md) |
| 41 | suite-wide compile-in-tests antipattern — convert to build-stage fixtures | tech-debt | testing | [0041-compile-in-tests-suite-wide.md](0041-compile-in-tests-suite-wide.md) |
| 42 | platform/std-header architecture fragile — recurring libc/std clashes (#27/#36/#38) | tech-debt | c-api | [0042-platform-header-architecture-fragility-libc-std-clashes.md](0042-platform-header-architecture-fragility-libc-std-clashes.md) |

Resolved issues live in [`archived/`](archived/).
