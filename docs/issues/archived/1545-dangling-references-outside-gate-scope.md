---
id: 1545
title: "Dangling references in files every gate's scope stops short of — including a book page red on `main`"
status: resolved
type: bug
area: [docs]
severity: low
found: 2026-09-28
resolved: 2026-09-28
related: [phase-472, 1085]
---

Each class has a gate; each gate's scope stops short of where these live.

- **`book/src/reference/c-api.md:72`** quotes `nros_executor_register_client()`,
  which the tree no longer defines. `check-book-identifiers` is **rc=1 on `main`**
  — VERIFIED — but it lives in `just/check/docs.just`, which no gating lane runs,
  so nothing surfaced it. The page last changed 2026-09-09.
- **24 dead `just` references** outside `check-doc-recipe-refs`'s scope, including
  the retired `build-zenohd` recipe — the gate's own motivating example — in
  `packages/api/nros-c/docs/*.md` and the nros-cpp equivalents, and 13 in
  `tests/README.md`.
- **RFC-0034** (`docs/design/0034-platform-layer-split.md:335`) defines
  `[issue 0006]: ../issues/0006-rtos-dual-heap.md`; the file moved to `archived/`,
  so 8 clickable links 404. Reference-style definitions are unread by
  `check-markdown-links` — issue 1085's shape.
- **6 dangling issue ids** across 11 citations outside `check-prose-issue-refs`'s
  directories (the root `justfile`, `.rs`, `.hpp`, workflows).
- **2 dead ledger citations** — `types.json` and `service.json` cite files that do
  not exist at those paths.

## Fix

Fix the references; widen each gate's scope (phase-472 W5). The book red is a
one-line doc fix and the cheapest item here.

## Resolution

Every reference named above is fixed. Four of the five gates now cover the
place where their class was found, so a recurrence fails instead of passing
unnoticed.

- **Book page.** `c-api.md` names `nros_executor_add_client()` and
  `nros_executor_add_action_client()`. The page also quoted
  `nros_executor_register_action_client()`. That name was not defined either,
  but it passed because the gate accepts "occurs" as "defined".
- **24 dead `just` references.** Each now names a recipe `just --list` shows
  (`just test qemu|esp32|zephyr|xrce`, `just native test-ros2|test-c`,
  `just xrce test-ros2|test-c`, `just docker build|test|test-qemu`,
  `just native zenohd`). Where a line names a retired recipe on purpose, it no
  longer reads as a runnable command. `just px4 test` was removed, because its
  crates were deleted.
- **RFC-0034** points at `archived/`. The widened gate also found archived
  issue 0167's `[0135]` definition, which resolved to `archived/archived/`.
- **11 citations of 6 ids** are fixed, as comment-only edits:
  - 0771 is 0773.
  - 0637 was PR #637, which is 1167.
  - One 0982 site is 1161.
  - 0982 and 0915 cite their fix commits.
  - 1110 points at phase-431 W6.
  - 1080 names its fix commits.
- **3 ledger citations.** The third was found by the widened gate.

Widened gates:

- `check-doc-recipe-refs` covers every tracked `.md`.
- `check-markdown-links` reads `[label]: target` definitions.
- `check-prose-issue-refs` covers the root `justfile`, `.github/`, and tracked
  C, C++ and Rust sources.
- `check-ledger-orphan-refs` resolves crate-relative paths by package name.

Each widened gate was mutation-tested: reverting the fix made it fail, and
the fixed tree passes.

**Left for phase-472 W5:**

- `check-book-identifiers` still accepts "occurs" as "defined", and no gating
  lane runs it.
- Ids with no issue file remain in `.config/capability-skip-baseline.txt`
  (0982), `.config/nextest.toml` (1165) and `ci/docker/` (0866). All are
  outside the widened scope.
