# Phase 208 — Book Starter Tutorial Audit

- **Goal:** Audit every "first-touch" tutorial under `book/src/getting-started/`
  and `book/src/start-here/` by strict-follow execution from a clean worktree.
  Surface drift between the book and the current tree (post-Phase 195 `nros`
  CLI, post-Phase 140 install-prefix removal, post-Phase 118 example collapse,
  post-Phase 169 dust-dds retirement) before a new user hits it.
- **Status:** active
- **Priority:** high — first-touch UX is the project's funnel; doc rot here
  costs every new contributor.
- **Depends on:** Phase 195 (`nros setup` canonical), Phase 203 (clean-rebuild
  baseline), Phase 197 (`just`→`nros` migration).

## Overview

Two-stage audit. Stage 0 = read-only sweep by the agent (me) of every starter
page, cross-checked against the current tree, with obvious doc errors fixed +
committed before any execution agents spawn. Stage 1 = 15 parallel
strict-follow execution agents, one tutorial each, isolated worktrees kept
for forensic inspection.

The point of Stage 0 is not to pre-empt findings — it's to avoid 15 agents
all flagging the same already-known doc breakage (e.g. a `just install-local`
reference, a path under `build/install/`, a `packages/codegen` mention).

## Tutorials in scope (15)

Linux first:

1. `start-here/choose-your-entry.md`
2. `start-here/setup-compared-to-ros2.md`
3. `getting-started/installation.md`
4. `getting-started/first-node-rust.md`
5. `getting-started/first-node-c.md`
6. `getting-started/first-node-cpp.md`
7. `getting-started/troubleshooting-first-10-min.md`

Embedded:

8. `getting-started/freertos.md`
9. `getting-started/integration-zephyr.md`
10. `getting-started/integration-nuttx.md`
11. `getting-started/threadx.md`
12. `getting-started/esp32.md`
13. `getting-started/integration-esp-idf.md`
14. `getting-started/integration-platformio.md`
15. `getting-started/bare-metal.md`
16. `getting-started/px4.md`

(15 active execution agents — `choose-your-entry` + `setup-compared-to-ros2`
fold into the read-only Stage 0 sweep since they have no commands to run.)

## Stage 0 — first-pass review (no agents)

Cross-check each page against the tree:

- Commands exist (`just <recipe>`, `nros <subcommand>`, `scripts/...`).
- Paths exist (`build/zenohd`, `build/qemu/bin`, `~/.nros/bin/nros`,
  `third-party/...`).
- No references to retired surfaces: `packages/codegen` submodule,
  `just install-local`, `build/install/`, `find_package(NanoRos)`,
  `rmw-dds`/dust-dds, per-RMW example dirs (Phase 118 collapse), the
  old per-`<rmw>/<case>/` Zephyr layout (Phase 168.6.C collapse),
  `set_wake_signal` (Phase 124.B replaced by `set_wake_callback`).
- Env vars accurate (`NROS_HOME`, `ZENOH_LOCATOR`, `ROS_DOMAIN_ID`,
  per-platform `*_DIR`).
- Provisioning surface matches Phase 195: `nros setup <board>`,
  `nros setup --rmw <rmw>`, `nros setup --tool <t>`,
  `scripts/install-nros.sh`.

Output: a single commit `docs(208): book starter audit — stage 0 fixes`
listing every drift fixed + the page it was on.

## Stage 1 — strict-follow execution agents

One agent per tutorial. Each agent:

- Spawns in its own git worktree (`isolation: worktree`) off current HEAD.
- Worktrees are **kept** after the audit (`tmp/book-audit/worktrees/<n>/`
  in the report) for inspection.
- Strict-follows every command verbatim. **No improvisation, no
  self-fixes, no edits to the book.** Read-only audit.
- Records: command, exit code, key stderr (≤ 5 lines), every drift between
  doc and reality.
- Rates issues: `BLOCKER` (can't proceed) / `FRICTION` (works after
  out-of-band knowledge) / `NIT` (typo, polish).
- Caps clarity: `CLEAR` / `VAGUE` / `MISSING`.
- Returns ≤ 400-word report, sections:
  `BLOCKERS / FRICTION / CLARITY / MISSING STEPS / WORKS`.
- Captures worktree path + last command + last exit code at the end.

### Per-batch wall-clock caps

Batches sequential. Agents within a batch run parallel.

| Batch | Agents | Cap | Notes |
|-------|--------|-----|-------|
| 1 (light Linux) | install, first-node-{rust,c,cpp}, troubleshoot-10min | 20 min | Cargo + zenohd reuse from host store + `~/.cargo` |
| 2 (QEMU light) | freertos, threadx, bare-metal | 30 min | QEMU pre-built in host store; small toolchains |
| 3 (NuttX) | nuttx | 45 min | Kernel rebuild per fixture |
| 4 (Zephyr) | zephyr | 60 min | west init + SDK (~5 GB if not cached) |
| 5 (ESP) | esp32, esp-idf | 60 min | ESP-IDF + xtensa toolchain |
| 6 (heavy) | platformio, px4 | 60 min | PX4 main is a big checkout |

**Cache reuse:** every worktree shares the host `~/.nros/sdk` store +
`~/.cargo` registry. SDKs already provisioned on the host are NOT
re-downloaded; `nros setup` is idempotent. New worktree only pays for its
own `target/` + per-platform `build/`.

## Stage 2 — synthesis

Aggregate the 15 reports into `tmp/book-audit/SUMMARY.md`:

- Per-tutorial severity matrix (BLOCKER / FRICTION / NIT counts).
- Cross-tutorial recurring patterns (e.g. recurring `nros setup`
  confusion, missing prereq, wrong path, outdated screenshot).
- Recommended doc edits — **not applied at this stage.** A follow-up
  phase (208.B) lands the edits once the user reviews the matrix.

## Acceptance

- [ ] Stage 0 commit landed + pushed.
- [ ] 15 execution-agent worktrees produced + their reports persisted at
      `tmp/book-audit/<tutorial>.md`.
- [ ] `tmp/book-audit/SUMMARY.md` written, severity matrix complete,
      recurring patterns called out.
- [ ] User has reviewed the matrix and signed off on which findings
      become Phase 208.B doc edits.

## Notes

- Read-only audit. Agents do not commit, do not push, do not modify the
  book. The only commits this phase lands are the Stage 0 sweep + the
  follow-up doc edits in 208.B once the matrix is approved.
- Worktrees are intentionally kept so the user can `cd` into a failing
  one and reproduce the exact state the agent saw.
- "Strict-follow" means the agent runs the literal command the book
  prints, even if a better invocation exists. The point is to catch
  what a new user would actually hit.
