# Phase 445 — one board choice generates the leaf's build configuration

**Status (2026-09-10). Opened; revised the same day to the colcon shape (RFC-0098 D1/D9); no work item started.** Implements
[RFC-0098](../design/0098-generated-leaf-build-config.md).

**Prior phases:** 341 (the board `cargo_config` projection), 331 (the
generated `<entry>_nros_selection` package), 412 (derived counts), 392 W5
(entity facts), 437 (board names state reach).

## Goal

A user picks a board in `system.toml`, runs `nros build` (or `nros sync` and
their own command), and gets an image. Nobody hand-writes a triple, a link
flag, a heap size, a pool size or an IP address into `Cargo.toml` or
`.cargo/config.toml`; a workspace is a directory of packages with no root build
file, like a colcon workspace; everything generated lives in `build/`,
`dist/` and `log/`; and no build leaves a tracked file modified.

## Work items

Ordered so no step deletes a value before its new home exists: gitignoring
`.cargo/` today would drop the only copy of 19 leaves' board triple and the
esp32 stack budgets.

- [ ] **W1 — `ZPICO_MAX_QUERYABLES` on the cargo road (RFC-0098 D7).
  ANSWERED; implemented by W3 + W4.** No open issue or PR wires it (checked
  2026-09-10; #779 covers `MAX_NODES` and the take buffer only). The consumer
  already computes the count from `NROS_DECLARED_SERVICE_SERVERS` +
  `NROS_DECLARED_INFRA_QUERYABLES` + `NROS_DECLARED_NODES`
  (`nros-zpico-build/src/runner.rs`) and `entity_facts::facts_from_model`
  produces them, so the facts are carried, never the count (issue 0460). The
  first revision's open question — a workspace member's sidecar is the
  workspace root's, shared by every image — is closed by D1/D9: each image's
  facts go in its own `build/<image>/nros-cargo.toml`. A single-package leaf
  gets its model in W3; the generator that writes the facts is W4. The esp32
  leaves' hand-set `ZPICO_MAX_QUERYABLES` goes when both have landed —
  acceptance a BUILD whose shim constant matches the hand-set value it
  replaces.
- [x] **W2 — complete the board descriptors (D4).** `[build] target` in every
  descriptor whose board has a Rust triple (mps2 first — the 19 hand-written
  leaves); `CC_<triple>`/`CFLAGS_<triple>` from the workspace configs; the
  hardware budgets (`NROS_HEAP_SIZE`, `ZPICO_SUBSCRIBER_LARGE_SIZE`,
  `NROS_SMOLTCP_MAX_UDP_SOCKETS`, `ESP_LOG`) and the netstack choice as
  `[knobs]`. Resolve the mps2 link-group disagreement between the descriptor
  and `examples/workspaces/{rust,realtime-rust}/.cargo/config.toml` by BUILD,
  not by picking one. Gate: a descriptor with a Rust triple and no
  `[build] target` fails.
  **Landed (2026-09-10, `feat/phase-445-w2-board-descriptors`):**
  - `[build] target` added to mps2-an385, mps2-an385-freertos, s32z270 and
    mps3-an536. `CC_thumbv7m_none_eabi` added to both mps2 boards; NuttX's
    `CC_`/`CFLAGS_` were already there, identical to the workspace copies.
    The 22 deploy leaves whose projection now supplies the triple drop their
    own `[build]`. That is forced, because sync's conflict check intersects
    keys; the value stays committed in the projection until W4/W6.
  - `[board.knobs]` is now READ: the CLI refused any `[knobs]` in a
    descriptor, so the RFC-0049 board rung existed only in tests.
  - esp32 `[board.knobs.net] max_udp_sockets = 2`, measured delivered: 2
    with the leaf's env line removed, 1 without the board facts.
  - NOT board facts, and not moved:
    - `NROS_HEAP_SIZE`: the leaf value equals the platform crate's default,
      and a rung would override `dds-heap`.
    - `ZPICO_NO_SMOLTCP`: a serial transport choice, set by 3 of 13 leaves on
      one board.
    - `ZPICO_SUBSCRIBER_LARGE_SIZE`: derived per image.
    - `ESP_LOG`: already in `cargo_config` `[env]`; read by esp-println, not a
      ladder knob.
  - Link groups: the workspace `-Tmps2_an385.ld --nmagic` is the FreeRTOS
    board's group, not a rival to bare-metal's `-Tlink.x`. Crossed builds fail
    both ways (`cannot find linker script`). Both workspace FreeRTOS images
    link and boot with the descriptor's `+ --gc-sections`. Gate:
    `check-board-build-target`.
  - Built after the change, one image per changed board:
    - mps2 bare-metal talker and mps2 FreeRTOS talker (both boot in QEMU);
    - esp32 talker;
    - NuttX ARM talker (boots);
    - both workspace FreeRTOS images (both boot);
    - `workspace-cpp-{mps3-an536,s32z270}-freertos` (mps3 boots to
      "Network ready").
- [ ] **W3 — `system.toml` in every single-package example (D3, D5, D8).** 178
  leaves (70 Rust, 52 C, 56 C++). `[image.X] board`, `[system] rmw/domain_id/locator`,
  network identity, `[[component]]` with entities where the board cannot be
  probed (issue 1265). `nros sync` resolves a single-package leaf from it; the
  `[package.metadata.nros.{entry,deploy.*,node,component}]` keys retire from the
  manifests. The two board spellings for one esp32 (`esp32-c3-baremetal` /
  `esp32-qemu`) collapse to one.
- [ ] **W4 — one generated settings file per image, under `build/` (D1, D6,
  D7).** `nros sync` / `nros build` write `build/<image>/nros-cargo.toml` (board
  `cargo_config` + per-image `target-dir` + resolved `[env]` incl. the entity
  facts + in-repo patch rows) and the generated entry
  `build/<coord>/<entry>/Cargo.toml` as its own cargo root. Stage 5 runs
  `cargo build --manifest-path … --config …`; the fixture lane builds through
  the same file and retires its explicit `--target` and per-invocation
  `NROS_DECLARED_*` exports. The per-leaf `.cargo/` writers (patch rows, the
  `nros-managed-{patch,env}.toml` sidecars, the `nros-board.toml` projection,
  their `include` bookkeeping) are deleted. Provisioning paths
  (`NROS_PLATFORM_*`) leave the leaves.
- [ ] **W5 — no workspace root build file (D9).** Stop generating
  `<ws>/Cargo.toml` (`rust`, `realtime-rust`, `features`, `launch`, `safety`,
  `sizing`) and retire `builder/cargo_root.rs`'s workspace emitter; delete the
  tracked root `.cargo/` in `rust` and `realtime-rust`; remove the root build
  files six templates track (five `CMakeLists.txt`, `multi-node-workspace`'s
  `Cargo.toml`); finish moving the legacy hand-written `src/*_entry` packages to
  generated entries (RFC-0065 D13).
- [ ] **W6 — delete and gate (D1, D2).** `git rm` the 47 leaf
  `.cargo/config.toml` files and the 34 `nros-board.toml` projections. Gates:
  refuse any tracked `examples/**/.cargo/*`; refuse a workspace-root
  `Cargo.toml`/`CMakeLists.txt` under `examples/workspaces/` and
  `examples/templates/`; `check-cargo-config-tracked` and
  `check-board-projections` retire with the files they guarded; the build
  preflight says `nros sync`. CLAUDE.md's 0457 / "never commit the include
  line" entries go with them.
- [ ] **W7 — the user flow in the book.** `nros build <image>`, and
  `nros sync` + `cargo build --config build/<image>/nros-cargo.toml` for a user
  driving cargo, for each board family.

## Acceptance

- A fresh clone, then `nros build <image>`, builds one image per board family —
  a BUILD, not a gate (#393). `nros sync` + plain `cargo build --config
  build/<image>/nros-cargo.toml` builds the same image.
- Switching a single-package example to another board is ONE line in
  `system.toml`; `git diff` afterwards shows that line only.
- `git status` is clean after `just ci gate` and after a fixture build.
- `find examples -path '*/.cargo/*'` returns nothing, and no directory under
  `examples/workspaces/` or `examples/templates/` has a root `Cargo.toml` or
  `CMakeLists.txt`.
- `rg '^\[package\.metadata\.nros\.(deploy|entry|node|component)' examples`
  returns nothing.

## Related

- Issue 1265 — the probe cannot run for cross-only leaves; D8's declaration is
  its workaround.
- Issue 1142 — a NuttX standalone example has no entity channel; W3 gives it
  one.
- Issues 0457, 0463, 0827, 1061 — the decisions this phase revisits or
  completes.
