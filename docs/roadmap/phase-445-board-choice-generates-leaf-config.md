# Phase 445 — one board choice generates the leaf's build configuration

**Status (2026-09-10). Opened; no work item started.** Implements
[RFC-0098](../design/0098-generated-leaf-build-config.md).

**Prior phases:** 341 (the board `cargo_config` projection), 331 (the
generated `<entry>_nros_selection` package), 412 (derived counts), 392 W5
(entity facts), 437 (board names state reach).

## Goal

A user picks a board in `system.toml`, runs `nros sync`, and builds with
whatever command they use. Nobody hand-writes a triple, a link flag, a heap
size, a pool size or an IP address into `Cargo.toml` or `.cargo/config.toml`,
and no build leaves a tracked file modified.

## Work items

Ordered so no step deletes a value before its new home exists: gitignoring
`.cargo/` today would drop the only copy of 19 leaves' board triple and the
esp32 stack budgets.

- [ ] **W1 — `ZPICO_MAX_QUERYABLES` on the cargo road (RFC-0098 D7).** No open
  issue or PR wires it (checked 2026-09-10; #779 covers `MAX_NODES` and the
  take buffer only). The CMake road already completes the count with
  `NROS_DECLARED_INFRA_QUERYABLES` from `nros ws entity-facts`; the cargo
  sidecar (`leaf_entity_env.rs`) withholds the knob as
  `NOT_DERIVED_NEEDS_INFRA_COUNT`. The consumer already computes the count
  from `NROS_DECLARED_SERVICE_SERVERS` + `NROS_DECLARED_INFRA_QUERYABLES`
  (`nros-zpico-build/src/runner.rs`), so the sidecar CARRIES those two facts
  from `entity_facts::facts_from_model` wherever the leaf has a model — it
  never states `ZPICO_MAX_QUERYABLES` itself (the consumer owns the cost,
  issue 0460). A leaf without a model keeps the withholding. Acceptance: a
  plain `cargo build` of a workspace entry leaf, with no `NROS_DECLARED_*` in
  the process environment, yields the same shim constant the fixture lane
  gets by exporting them; `check-declared-fact-carriers` names the facts on the
  sidecar road. (A single-package leaf has no model until W3 — its hand-set
  `ZPICO_MAX_QUERYABLES`, e.g. the esp32 talker's `2`, is removed THERE.)
- [ ] **W2 — complete the board descriptors (D4).** `[build] target` in every
  descriptor whose board has a Rust triple (mps2 first — the 19 hand-written
  leaves); `CC_<triple>`/`CFLAGS_<triple>` from the workspace configs; the
  hardware budgets (`NROS_HEAP_SIZE`, `ZPICO_SUBSCRIBER_LARGE_SIZE`,
  `NROS_SMOLTCP_MAX_UDP_SOCKETS`, `ESP_LOG`) and the netstack choice as
  `[knobs]`. Resolve the mps2 link-group disagreement between the descriptor
  and `examples/workspaces/{rust,realtime-rust}/.cargo/config.toml` by BUILD,
  not by picking one. Gate: a descriptor with a Rust triple and no
  `[build] target` fails.
- [ ] **W3 — `system.toml` in every single-package example (D3, D5, D8).** 178
  leaves (70 Rust, 52 C, 56 C++). `[image.X] board`, `[system] rmw/domain_id/locator`,
  network identity, `[[component]]` with entities where the board cannot be
  probed (issue 1265). With a model per leaf, W1's sidecar facts reach every
  single-package leaf, so the hand-set `ZPICO_MAX_QUERYABLES` in the esp32
  leaves goes here — acceptance a BUILD whose shim constant matches the
  hand-set value it replaces. `nros sync` resolves a single-package leaf from it; the
  `[package.metadata.nros.{entry,deploy.*,node,component}]` keys retire from the
  manifests. The two board spellings for one esp32 (`esp32-c3-baremetal` /
  `esp32-qemu`) collapse to one.
- [ ] **W4 — sync writes ONE generated `.cargo/config.toml` (D1, D6).** Board
  `cargo_config` + resolved `[env]` + in-repo patch rows; no `include`; the
  `nros-board.toml` projection folds in. The board crate arrives through the
  generated `<entry>_nros_selection` package. Provisioning paths
  (`NROS_PLATFORM_*`) leave the leaves.
- [ ] **W5 — untrack (D1, D2).** Per-example `.gitignore` of `.cargo/`;
  `git rm --cached` of the 47 configs and 34 projections. Gates: a new
  refusal of any tracked `examples/**/.cargo/*`; `check-cargo-config-tracked`
  and `check-board-projections` rewritten for the inverted rule; the build
  preflight (`_require-leaf-includes`) says `nros sync`. CLAUDE.md's
  0457 / "never commit the include line" entries are retired with it.
- [ ] **W6 — the user flow in the book.** `nros sync`, then the build command
  of the user's choice, for each board family.

## Acceptance

- A fresh clone, `nros sync`, then `cargo build` / `cmake` builds one leaf per
  board family — a BUILD, not a gate (#393).
- Switching a single-package example to another board is ONE line in
  `system.toml` plus `nros sync`; `git diff` afterwards shows that line only.
- `git status` is clean after `just ci gate` and after a fixture build.
- `rg '^\[package\.metadata\.nros\.(deploy|entry|node|component)' examples`
  returns nothing.

## Related

- Issue 1265 — the probe cannot run for cross-only leaves; D8's declaration is
  its workaround.
- Issue 1142 — a NuttX standalone example has no entity channel; W3 gives it
  one.
- Issues 0457, 0463, 0827, 1061 — the decisions this phase revisits or
  completes.
