---
id: 172
title: "Onboarding doc-drift batch — dead verbs, wrong paths, missing CLI reference entries, misleading prerequisites, three unreconciled bootstrap routes"
status: resolved
type: tech-debt
area: docs
related: [phase-222, phase-277]
resolved_in: "2026-07-09 onboarding drift sweep"
---

## Problem (summary)

Thirteen string-level drift items across the onboarding surfaces (AGENTS.md,
README, `book/src/reference/cli.md`, the activate hints, `nros --help`, and
the `nros setup` listing output), collected by the 2026-07-09 UX audit.

## Resolution (per item)

1. **AGENTS.md dead verbs** — "`nros build`/`deploy` lazy-install…" rewritten:
   lazy-install happens on the platform build path via `setup::ensure_tools`
   (`NROS_NO_AUTO_SETUP` opt-out); the verbs were retired in Phase 222.
2. **`examples/threadx-riscv64/` path** — fixed to
   `examples/qemu-riscv64-threadx/` in `introduction.md`,
   `reference/supported-boards.md`, and `threadx.md`'s brace-shorthand.
   (`creating-examples.md`'s cell is a board *label*, not a path — left.)
3. **`scripts/install-nros.sh` mentions** — NO CHANGE NEEDED: both
   `activate.sh` and `justfile` comments correctly describe the script as
   *retired* historical context; they don't instruct anyone to run it.
4. **cli.md missing verbs** — added entries for `generate-rust` (the
   side-effect-free primitive, `--generate-config` alias noted),
   `generate-px4-msgs`, and `codegen` (marked internal build-tool interface).
5. **"There is no `nros release` verb"** — softened: a hidden maintainer-only
   verb exists behind the `release` cargo feature; default builds lack it.
6. **`generate rust` vs `generate-rust`** — README keeps the user verb
   (`nros generate rust`); cli.md's new `generate-rust` entry + the vs-just
   table state the relationship (and that `nros sync` is the usual one-shot).
7. **`nros --help` quick-start** — added the `nros sync` line, completing the
   scaffold → codegen → build funnel (verified in the built binary).
8. **ROS 2 "(Optional)"** — README now says required for codegen/CycloneDDS/
   interop; only the pre-generated native Rust demo runs without it.
9. **cmake "(Optional)"** — now "required for the C/C++ examples".
10. **Submodule trap** — the README "already have cargo" one-liner, both
    activate first-run hints, and cli.md's Install alternates all prepend
    `git submodule update --init packages/cli/third-party/ros-launch-manifest`.
11. **`FREERTOS_PORT` implicit dependency** — README's activate step now
    states it wires the SDK env (`FREERTOS_PORT` named) and that skipping it
    is the top first-build failure.
12. **Three divergent bootstrap routes** — README, cli.md §Install, and both
    activate hints now present the same recommended-plus-two-alternates set.
13. **`nros setup --list`/`--licenses` on stderr** — moved to stdout
    (pipeable), progress/diagnostics stay on stderr; verified
    `nros setup --list 2>/dev/null` emits the listing.

Verified: CLI rebuilds clean (`cargo build --release --manifest-path
packages/cli/Cargo.toml`), no new clippy warnings, nightly-fmt clean,
`bash -n` / `fish -n` pass on the activate files, mdbook build green.
