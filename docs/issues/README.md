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
| 8  | Two-copy receive path and static buffer pre-allocation at scale       | tech-debt   | rmw    | [0008-two-copy-receive.md](0008-two-copy-receive.md) |
| 21 | Borrowed (zero-copy) message views for C and C++                      | enhancement | codegen| [0021-cpp-c-borrowed-views.md](0021-cpp-c-borrowed-views.md) |
| 24 | esp32 .bss overflows DRAM — Phase 231 size-class receive buffers too large | bug       | build  | [0024-esp32-dram-overflow-size-class-buffers.md](0024-esp32-dram-overflow-size-class-buffers.md) |
| 25 | host-integration lane fails native action/c_xrce/bridge tests — fixtures not staged | bug | build | [0025-host-integration-native-fixtures-unstaged.md](0025-host-integration-native-fixtures-unstaged.md) |
| 27 | nros-c posix platform headers fail to compile under gcc 14 (riscv NuttX) | bug       | c-api  | [0027-nros-c-posix-headers-gcc14.md](0027-nros-c-posix-headers-gcc14.md) |

Resolved issues live in [`archived/`](archived/).
