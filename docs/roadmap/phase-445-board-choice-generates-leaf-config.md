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
  **W3b landed (2026-09-11, `feat/phase-445-w3b-convert-leaves`) — every
  SINGLE-PACKAGE leaf; the box stays open for the workspace members below.**

  What converted:
  - Rust: 42 examples and 3 `nros-tests` bins.
  - C: 44 leaves. C++: 43 leaves.
  - The `action-raw-goal-probe` C bin.
  - With the 11 W3 pilots, 144 leaves resolve through `system.toml`. Each one
    passes `nros sync` with no deprecation line, and `nros ws leaf-system` and
    `nros ws entity-facts --bringup-dir` both answer.

  Supporting changes the conversion needed:
  - `[[component]]` gained `dispatch` (issue 1278, resolved). The 15 leaves
    that set it keep it, including the W3 pilot talker, which had dropped it.
  - `[package.metadata.nros.entry] deploy` is optional. RTIC leaves keep the
    table for `node_pkgs`.
  - `rv-virt-threadx`, the RFC-0093 name, is now a descriptor name, and every
    ThreadX RISC-V leaf uses it.
  - A Zephyr package with `system.toml` beside it is a self-pkg bringup.

  The `<nano_ros deploy= board= rmw=/>` tuple is RETIRED and REFUSED in every
  reader and producer:
  - the cmake reader and the `cargo-nano-ros` reader;
  - `nros setup --workspace`;
  - the `nros new` scaffolder, which emitted it;
  - colcon;
  - two board gates that read it.

  Gate: `check-leaf-deployment-spelling`.

  NOT done in W3b, and why:
  - `leaf_system::from_manifest`, the Rust manifest fallback, was NOT deleted:
    workspace entries still carried the retiring tables. **W5 deleted it** (see
    below); the acceptance rg is issue 1289.

  Built in this worktree, each family by its own `just` lane, with 0
  retired-key deprecation lines in any build log:
  - native: Rust listener and `entry-poc`, C listener, C++ service-server;
  - mps2 bare-metal: every Rust leaf, including RTIC, XRCE and serial;
  - FreeRTOS: all 6 Rust leaves, plus C and C++;
  - ThreadX-Linux: Rust, C and C++;
  - ThreadX RISC-V: C and C++, zenoh and Cyclone rows, 17 executables;
  - NuttX ARM: Rust, C and C++. NuttX RISC-V: C.

  Probe-less board: mps2 `talker-rtic`'s generated zpico `shim_constants.rs`
  is byte-identical to a twin built from the pre-conversion manifest.

  NOT built: Zephyr. The west workspace's `nano-ros` module is a symlink to
  another checkout, so a west build from here would compile that checkout,
  not this one. Its leaves resolve (`nros sync`, `nros ws leaf-system`).
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
  **W5 remainder landed (2026-09-11, `feat/phase-445-w5-generated-entries`)**,
  on top of #880's root/settings work:
  - `examples/workspaces/rust/src/esp32_entry` deleted; `[image.esp32]`
    generates it. `esp-hal` joined the esp32 descriptor's `[board.entry]
    crate_root_deps`, the locator moved to the image, and the generator stopped
    naming the umbrella's `platform-bare-metal` marker on `nros-platform` (no
    such feature; the board crate selects `platform-esp32-qemu`).
  - A workspace entry's deployment is the image that claims it
    (`leaf_system::for_entry`, RFC-0098 amendment); the generated entry writes
    no `deploy`. `from_manifest` and its call are deleted; the retired keys are
    refused by the reader, `nros check` (`entry-deploy-retired`) and
    `check-leaf-deployment-spelling`, which stopped skipping workspace
    manifests (their node/component tables are counted — issue 1289).
  - The eight Rust Zephyr entries and every nros-tests workspace fixture lost
    their manifest deployment keys (bringup images, or a leaf `system.toml`
    where the fixture has no bringup or tests a Form-1 `nros::main!()`); the
    Zephyr entries stay hand-written west apps (issue 1288) and were NOT built
    (the west module symlinks another checkout, issue 1253).
  - `nros build` with no bringup builds package by package (RFC-0065 D1); the
    five template root `CMakeLists.txt` and the three C/C++ `robot_entry`
    packages are deleted, `check-no-tracked-workspace-roots` covers
    `examples/templates/`, and `nros new --workspace --lang cpp` scaffolds no
    root and no entry.
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
