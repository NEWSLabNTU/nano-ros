---
id: 1278
title: "`[package.metadata.nros.node] dispatch` has no home in system.toml, so a
  leaf converted by phase-445 W3 loses its dispatch strategy"
status: resolved
type: tech-debt
area: cli, examples
severity: low
related: [issue-1265]
resolved_by: phase-445 W3b
---

## What happens

Phase-445 W3 (RFC-0098 D3/D5/D8) moves a single-package leaf's node declaration
from `[package.metadata.nros.node]` in `Cargo.toml` to a `[[component]]` row in
the `system.toml` beside it. `[[component]]` (`SystemComponentEntry`) carries
`pkg`, `class`, `name`, `group_tiers`, `params`, `params_files` and — since W3 —
`entities`. It has no `dispatch`.

`dispatch = "inline" | "deferred" | "from_isr"` (phase-216 A.5) is set on 12
single-package leaves today (census: the RTIC and bare-metal MPS2 examples,
`rg -n '^dispatch' -g Cargo.toml examples`). Its only reader is
`cmd/check_workspace.rs`'s (framework x strategy) lint, which reads the
`[package.metadata.nros.node]` table. On a converted leaf that table is gone, so
the lint silently has nothing to check, and the strategy the author chose is no
longer recorded anywhere.

The W3 pilot `examples/mps2-an385-baremetal/rust/talker` was such a leaf
(`dispatch = "deferred"`); it builds unchanged, because nothing at build time
consumes the key.

## Why it matters

The remaining leaves are meant to be converted mechanically. For these 12 the
conversion cannot be lossless until the key has a destination.

## Direction

Add `dispatch` to `[[component]]` (nano-ros-owned `SystemComponentEntry`; the
resolver's `[[component]]` reader is lenient, so it costs rlm nothing), expose it
through `nros_orchestration_ir::leaf_system::LeafComponent`, and make the
`check_workspace` lint read it from there. Acceptance: the mps2 bare-metal
talker states `dispatch = "deferred"` in its `system.toml` and `nros check`
reports it exactly as it did from the manifest.

## Resolution (phase-445 W3b)

Done as directed:

- `SystemComponentEntry.dispatch` (typed schema) and `LeafComponent.dispatch`
  (the one reader, including its manifest fallback) carry the key.
- `check_workspace::read_node_dispatch_strategy` reads it from a package's
  `system.toml` `[[component]]` before it falls back to
  `[package.metadata.nros.node] dispatch`.
- Workspace discovery and the manifest-to-system synthesis carry it too.

All 15 leaves that set it now state it in `system.toml`:

- the 12 RTIC and bare-metal MPS2 examples converted in W3b;
- `qemu-baremetal-main-e2e` and `rtic-run-plan-e2e`;
- the W3 pilot `mps2-an385-baremetal/rust/talker`, which had dropped it and
  now has it back.

**The acceptance is met, and the thing it measures is narrower than the issue
assumed.** The unit test `system_toml_dispatch_reads_like_the_manifest_key`
shows the lint's reader returns the same strategy from either spelling:
inline, deferred, from_isr, the Inline default, and a typo that is refused
naming the file and the key. Since the matrix consumes nothing else, equal
answers mean equal reports.

But the lint walks an ENTRY's path-dependencies for Node packages. A
single-package leaf is its own entry and never a path-dependency of another,
so its own strategy was never linted, before this change or after it. So on
these 14 leaves `nros check` reports nothing, exactly as it did from the
manifest. The strategy is now recorded without loss. Linting a leaf's own node
against its board's framework would be a new rule, not a repair.
