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

- [ ] **W1 — `ZPICO_MAX_QUERYABLES` on the cargo road (RFC-0098 D7). Folded
  into W3; one question left open.** No open issue or PR wires it (checked
  2026-09-10; #779 covers `MAX_NODES` and the take buffer only). The consumer
  already computes the count from `NROS_DECLARED_SERVICE_SERVERS` +
  `NROS_DECLARED_INFRA_QUERYABLES` + `NROS_DECLARED_NODES`
  (`nros-zpico-build/src/runner.rs`); the facts come from
  `entity_facts::facts_from_model`. So the cargo road only has to CARRY those
  facts, never state the count (issue 0460). Where it can carry them was
  measured, and it is narrower than the first draft said:
  - **Single-package leaf** — it is its own patch authority, so the `[env]`
    sidecar `nros sync` writes is its own. With a model (W3) the sidecar
    carries the facts and `NOT_DERIVED_NEEDS_INFRA_COUNT` goes. This is done
    IN W3, and the esp32 leaves' hand-set `ZPICO_MAX_QUERYABLES` goes with it.
  - **Workspace member — OPEN.** `find_patch_authority` walks to the cargo
    workspace root, so a member's sidecar is the ROOT's, shared by every image
    in the workspace (`native`, `esp32`, `freertos`, each with its own launch
    file and model); and cargo reads `.cargo/` from the invocation's CWD, not
    per package. Per-image facts therefore cannot live in one `[env]`. Today
    they reach cargo as PROCESS env, one invocation per image — `nros build`
    and `workspace-fixtures-build.sh` both do this. A plain `cargo build -p
    <entry>` from the root gets the consumer's safe fallback (both families
    assumed, plus headroom), which `nros build`'s own comment measured as a
    DRAM overflow of 8,804 B on `esp32_entry`. Two candidate answers, not yet
    chosen: (a) the facts go in the ENTRY's own `.cargo/` beside its board
    projection, valid when cargo runs from the entry directory — the same
    condition its `[build] target` already needs; (b) a multi-image workspace
    is built by `nros build`, and RFC-0098 D2's "any build command" applies to
    single-package leaves. The fixture lane builds from the workspace root with
    an explicit `--target` today, so it reads neither.
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
