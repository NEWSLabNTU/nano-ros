---
id: 172
title: "Onboarding doc-drift batch — dead verbs, wrong paths, missing CLI reference entries, misleading prerequisites, three unreconciled bootstrap routes"
status: open
type: tech-debt
area: docs
related: [phase-222, phase-277]
---

## Problem

Accumulated small drift across the onboarding surfaces (2026-07-09 audit). Each
item is a string-level fix; batched here so one pass closes them all.

**Dead / wrong references:**

1. `AGENTS.md:110` — "`nros build`/`deploy` lazy-install a board's tools…":
   both verbs were removed in Phase 222 (`nros doctor` itself lints
   `system.toml` for their leftovers, `doctor.rs:413-437`). The lazy-install
   mechanism (`setup::ensure_tools`, `setup.rs:592`) survives but is reached
   from the platform build path, not those verbs.
2. `examples/threadx-riscv64/` → real dir is `examples/qemu-riscv64-threadx/`:
   `book/src/introduction.md:90`, `book/src/reference/supported-boards.md:25`,
   `book/src/internals/creating-examples.md:198`,
   `book/src/getting-started/threadx.md:52`.
3. Stale `scripts/install-nros.sh` mentions (script retired) in
   `activate.sh:44` and `justfile:2043`; live scripts are `bootstrap.sh` +
   `install-nros-prebuilt.sh`.

**CLI reference gaps (`book/src/reference/cli.md`):**

4. Missing entries for `generate-px4-msgs` and the top-level `codegen` verb
   (`resolve-deps`/`cyclonedds-descriptors`/`entry`); `generate-rust` is only
   mentioned inline, never given its own entry.
5. `cli.md:258` claims "There is no `nros release` verb" — a
   `#[cfg(feature = "release")]`-gated variant exists (`cmd/mod.rs`).
6. README mixes `nros generate rust` (space) and `nros generate-rust` (hyphen)
   — **different code paths** (`generate::run` has patch side-effects,
   `run_rust` does not); pick one canonical spelling and state the difference
   once.
7. `nros --help` quick-start (`nros-cli/src/main.rs:15-23`) omits
   `generate-rust`/`sync` — the very commands a first Rust user needs right
   after scaffolding.

**Misleading prerequisites (root `README.md`):**

8. ROS 2 Humble listed "(Optional)" (line 47) — required for
   codegen/`nros sync`/cyclonedds/every interop path; only the pre-generated
   native Rust pair escapes. `activate.sh:37` warning is the only hint.
9. cmake "(Optional) for C examples" (line 48) while the C quick-start
   (lines 106-115) is presented first-class.
10. The "already have cargo" one-liner (line 64) omits the
    `packages/cli/third-party/ros-launch-manifest` submodule init →
    `failed to read …/types/Cargo.toml` (the failure `bootstrap.sh:184`
    documents but the README doesn't).
11. `FREERTOS_PORT` defaults only via sourced `activate.sh`
    (`just/sdk-env.just:4`) — implicit dependency, panics in fresh shells /
    copy-outs.

**Structural:**

12. Three bootstrap routes (bootstrap.sh / cargo build / install-prebuilt)
    enumerated differently by README, `cli.md` §Install, and the `activate.sh`
    first-run hint — consolidate to one "recommended + alternatives" block
    shared verbatim.
13. `nros setup --list`/`--licenses` output goes to stderr (`setup.rs`) —
    unpipeable; move listings to stdout, keep diagnostics on stderr.

## Fix direction

One doc-sweep PR for items 1-12 (mechanical); item 13 is a small CLI change.
