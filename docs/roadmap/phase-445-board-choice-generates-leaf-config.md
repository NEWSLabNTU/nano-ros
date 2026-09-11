# Phase 445 — one board choice generates the leaf's build configuration

**Status (2026-09-11). Every wave is WRITTEN; the phase is NOT complete, and
two of its five acceptance bullets are measured NOT MET.** W1, W2, W3b, W4b, W5
and W6 landed. W7 is PR #926. W3's single-package half landed as W3b; its
workspace members moved under W5, so the box stays open rather than claiming
work that changed hands. The tree now has no `examples/**/.cargo/` at all and no
workspace root build file, both gated.

**What is not true yet, and why the phase stays here rather than in
`archived/`.** The headline — pick a board in `system.toml`, run `nros build`,
get an image — holds for Rust and not for the other two roads:

* **issue 1305** — the one-line board switch is FALSE for a single-package RUST
  leaf. It is its own entry, so it still names its board crate in
  `[dependencies]`; RFC-0098 D6's generation reaches the generated WORKSPACE
  entry only. Everything else (triple, link group, runner, cross compiler) does
  follow the one line.
* **issue 1296** — `nros build` does not resolve a single-package C/C++ leaf
  (the synthesised bringup is named for the directory, the cmake driver asks for
  `[system] name`). Those leaves keep their `cmake` pair, which is green.
  Issue 1308 is the same seam from the other side.

Both were found by RUNNING the documented commands (W7), not by reading, and
both are recorded against the acceptance bullets they falsify rather than
ticked. Archiving before they are answered would assert a claim the tree
contradicts — the stale-status class phase-413 exists to remove.
Opened 2026-09-10; revised the same day to the colcon shape (RFC-0098 D1/D9). Implements
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

  **Landed for single-package leaves with W4b (2026-09-11,
  `feat/phase-445-w4b-single-package-settings`). The acceptance as written did
  NOT hold, and the difference is the finding:**
  - The facts reach the image through `build/<image>/nros-cargo.toml` (and the
    per-leaf sidecar, until W6). `NOT_DERIVED_NEEDS_INFRA_COUNT` is now
    `QUERYABLES_DERIVED_BY_CONSUMER`: the leaf road still never states the
    count, and the pool-floor gate reads either abstention spelling.
  - The model alone was not enough. It abstains on
    `NROS_DECLARED_SERVICE_SERVERS` for every leaf without a contract (all of
    them), which leaves the consumer at its 8-slot headroom — WORSE than the
    hand-set 2. The application half therefore comes from the leaf's own
    inventory (probe, or the D8 `entities` declaration), the source every other
    derived pool on this road already uses.
  - A single-package leaf picks the parameter/lifecycle families with CARGO
    features, which its model cannot see (`native/rust/lifecycle-node` enables
    `lifecycle-services` with no `[system] features`). The infrastructure fact
    is the model's UNIONED with the manifest's; without that the table would
    be five slots short at boot.
  - Measured on the esp32 talker, both `nros-relwithdebinfo`, same settings
    file, only the leaf `[env]` line removed:
    `ZPICO_MAX_QUERYABLES` 2 -> **1**, `ZPICO_MAX_SUBSCRIBERS` 1 -> 1,
    `.bss` 219,664 -> 215,160 B, `.stack` 94,248 -> 98,752 B (+4,504 B, one
    `SERVICE_BUFFERS` slot). 1 is the demand: no service server and neither
    family, floored at one by the consumer. The hand-set 2 was headroom chosen
    while nothing could state the application count ("2, not 0 … this row is
    where the image's own author can state it"). Both knobs are removed from
    the talker and listener `.cargo/config.toml`.
  - NOT removed: `ZPICO_MAX_QUERYABLES = "2"` on the `workspace-rust-esp32`
    fixture row. That image is built by the WORKSPACE road, whose facts come
    from the model alone (no inventory fill), so removing it derives 8 — the
    8,804 B DRAM overflow the row comment records. Also not removed:
    `logging-smoke-esp32-qemu`'s hand-set value (no `system.toml`, so no
    facts), and the talker-xrce `NROS_XRCE_MAX_*` rows (the leaf road derives
    no XRCE pool).
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
  **W4b landed (2026-09-11, `feat/phase-445-w4b-single-package-settings`) —
  single-package leaves; the per-leaf `.cargo/` writers are KEPT for W6.**
  - One writer: `builder/cargo_config.rs` (W4's) now also takes path-valued
    `[env]` rows and a header command, and always carries the three
    `nros-cargo-profile` presets. `cmd/leaf_settings.rs` resolves a leaf (a
    `[package]` `Cargo.toml` with `system.toml`, a cargo-driven board) and
    writes `<leaf>/build/<image>/nros-cargo.toml`. `nros sync` writes it last;
    `nros build` in a leaf writes it and hands over
    `cargo build --manifest-path <leaf>/Cargo.toml --config <leaf>/build/<image>/nros-cargo.toml`.
    `nros ws leaf-system` prints `NROS_LEAF_SETTINGS=<path>`.
  - Layers, later wins: board `cargo_config` < derived pools + entity facts <
    board facts (`board-facts`, which the lane used to export per row) < the
    `[env]` rows the leaf's tracked `.cargo/config.toml` still AUTHORS. That
    last layer is TRANSITIONAL — see below.
  - Grandparent-relative paths: the writer already writes against the file's
    grandparent, here `<leaf>/build/`.
  - The working directory: the `nros-*` profiles now travel in the file, so
    they no longer need cargo to start inside the checkout. The leaf's own
    `.cargo/config.toml` is the other half. Its board-projection `include`
    repeats the file's `[target.<triple>] rustflags`, and cargo JOINS arrays
    across config files. Measured on the mps2 bare-metal talker:
    `rust-lld: error: memory.x:19: region 'FLASH' already defined`. So cargo
    runs from the directory ABOVE the leaf: ancestor configs still apply, the
    leaf's own does not. Once W6 deletes it, running from the leaf is
    equivalent.
  - Fixture lane: a row whose leaf has `system.toml` builds through the file
    (`scripts/build/leaf-settings.sh`, shared with the staleness probe). It no
    longer passes `--target` (the six FreeRTOS rows) and no longer exports
    `board-facts`. The group `--target-dir` still wins, so no artifact path
    moves. `fixtures-manifest.py builds_through_leaf_settings` is the one
    predicate. The builder FAILS when that predicate holds but the CLI reports
    no settings file.
  - Built, one leaf per family with Rust examples, each through `nros build`
    AND through `nros sync` + plain cargo from the leaf's parent. The plain
    build is `Fresh` with 0 units recompiled, i.e. the same configuration.
    Families: native, mps2 bare-metal, mps2 FreeRTOS, esp32, NuttX ARM,
    ThreadX-Linux (all talkers), plus the esp32 listener.
  - **W6 can now delete, per leaf:** `.cargo/nros-board.toml`, the
    `nros-managed-{patch,env}.toml` sidecars and the `include` bookkeeping. It
    must first re-home the authored `[env]` rows the settings file carries:
    - esp32: `NROS_EXECUTOR_ARENA_SIZE`, `NROS_SMOLTCP_MAX_UDP_SOCKETS`,
      `ZPICO_SUBSCRIBER_LARGE_SIZE`;
    - mps2 serial and XRCE: `NROS_HEAP_SIZE`, `NROS_LINK_IP`,
      `ZPICO_NO_SMOLTCP`, the `NROS_XRCE_*` sizes;
    - FreeRTOS: `NROS_PLATFORM_{FREERTOS_SRC,CFFI_INCLUDE}` (provisioning,
      already exported by `sdk-env.just`).

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
- [x] **W6 — delete and gate (D1, D2).** `git rm` the 47 leaf
  `.cargo/config.toml` files and the 34 `nros-board.toml` projections. Gates:
  refuse any tracked `examples/**/.cargo/*`; refuse a workspace-root
  `Cargo.toml`/`CMakeLists.txt` under `examples/workspaces/` and
  `examples/templates/`; `check-cargo-config-tracked` and
  `check-board-projections` retire with the files they guarded; the build
  preflight says `nros sync`. CLAUDE.md's 0457 / "never commit the include
  line" entries go with them.

  **Landed (2026-09-11, `feat/phase-445-w6-delete-and-gate`).** 81 tracked
  files deleted: 45 `examples/**/.cargo/config.toml`, 33 example board
  projections, and the 3 projections that lived outside `examples/` (two
  `nros-tests` bins and the `n_board_agnostic_run_plan` freertos entry — all
  three already state their board in `system.toml`, so they take the same
  generated settings file). Plus the 3 now-pure-sync configs beside them.

  Where each `[env]` row went — measured, not reasoned:

  | knob | leaves | home | how proved |
  |---|---|---|---|
  | `NROS_PLATFORM_FREERTOS_SRC`, `NROS_PLATFORM_CFFI_INCLUDE` | 6 | nowhere — `just/sdk-env.just` already exports both | the FreeRTOS images build with the rows gone |
  | `NROS_LOCAL_IPV4`, `NROS_LOCAL_IPV4_BYTES` | 6 | nowhere — DEAD | zero readers in the tree; the last one (`nros-rmw-dds/build.rs`) was deleted with its crate in `f3f88cbaa`, 2026-05-19 |
  | `NROS_SMOLTCP_MAX_UDP_SOCKETS` | 2 | already `[board.knobs.net] max_udp_sockets` (W2) | a duplicate; the constant is unchanged with the row gone |
  | `NROS_EXECUTOR_ARENA_SIZE` | 2 | `[board.knobs.executor] arena_size = 16384` | same value on every image of the board, and it is about the board's DRAM |
  | `ZPICO_SUBSCRIBER_LARGE_SIZE` | 2 | `[image.<id>] env` on the LISTENER only | the talker derives `ZPICO_MAX_LARGE_SUBSCRIBERS = 0`, so its row sized a zero-byte pool; only the listener's unbounded `std_msgs/String` makes the derivation refuse |
  | `NROS_HEAP_SIZE` | 3 | nowhere — DEAD | `131072` IS `DEFAULT_HEAP_SIZE` (`128 * 1024`) in `nros-platform-mps2-an385`, and none of the three leaves enables `dds-heap` / `link-tls` |
  | `ZPICO_NO_SMOLTCP`, `NROS_LINK_IP` | 3 | DERIVED from `[image.<id>] transport = "serial"` | the same rule `PlanBuildOptions::drops_ip_link` already states one layer up |
  | `NROS_XRCE_STREAM_HISTORY`, `NROS_XRCE_SUBSCRIBER_RING_DEPTH` | 1 | nowhere — DEAD | both equal the default `packages/rmw/xrce/xrce-config.txt` already states (4 and 1) |
  | the other five `NROS_XRCE_*` | 1 | `[image.<id>] env` | not board facts (the board's other twelve images want the defaults) and not derivable (the XRCE pools take no part in RFC-0098 D7's derivation) |

  New in the schema: **`[image.<id>] env`, the RFC-0049 APP rung**, read by the
  one deployment reader (`leaf_system`) and landing in the settings file's
  `[env]` WITHOUT `force`, so a lane still outranks it. And
  `[image.<id>] transport` is now VALIDATED against the three link kinds —
  `talker-xrce` had named its RMW there for four phases and nothing read the
  key, so nothing said so.

  Deleted from the CLI: `BOARD_CONFIG_FILE` and the whole projection
  writer/checker (`project_board_config`, `render_board_config`,
  `board_projection_conflicts`, `render_board_include`, `BoardProjection`,
  `write_board_projection`, `render_board_projection_body`,
  `check_board_projection`, `project_board_configs{,_with}`), the
  `nros ws check-board-projections` verb, `mod board_projection_tests`,
  `MANAGED_ENV_FILE` + `render_leaf_env_sidecar` (issue 0827's derived `[env]`
  sidecar — layer 2 of the settings file now), and `authored_leaf_env`, the
  transitional layer W4b left for this item.

  NOT deleted, and why: `write_patch_config` and `MANAGED_PATCH_FILE`. The
  patch AUTHORITY for a workspace is `<ws>/.cargo/config.toml` (gitignored),
  and a leaf that a plain `cargo` or the metadata probe runs INSIDE still
  resolves its registry-named nros crates through one. The six Zephyr Rust
  leaves are the case with no alternative: west drives their cargo through
  zephyr-lang-rust's `rust_cargo_application`, which passes no `--config`
  (issue 1288). Their files are untracked now and sync regenerates them
  gitignored, which is what the gate asks; making Zephyr take a `--config`
  would need a west build to accept it, and no west build is possible from an
  agent worktree (the module is a symlink to another checkout — W3b's note).
- [x] **W7 — the user flow in the book.** `nros build <image>`, and
  `nros sync` + `cargo build --config build/<image>/nros-cargo.toml` for a user
  driving cargo, for each board family.
  **Landed (2026-09-11, `docs/phase-445-w7-book`, on top of W4b + W5.)**

  41 book pages, 9 `docs/guides`/`docs/reference` files, the root `README.md`,
  167 example READMEs (153 of them through `scripts/docs/gen-example-readmes.py`,
  which gains `--force`) and the zenoh probe's verifier. Every retired surface
  is gone from the user track: a hand-written leaf `.cargo/config.toml` or
  `.cargo/nros-board.toml`, `[package.metadata.nros.{entry,deploy.*,node,
  component}]`, the `package.xml` `<nano_ros deploy= board= rmw=/>` tuple, a
  workspace-root build file (generated or otherwise), `cargo`/`cmake` shown
  with no `nros sync`, a hand-passed `--target`, `-p <entry>`,
  `nros codegen-system`, `-DNROS_RMW=`/`-DNANO_ROS_BOARD=`, and `just` on a
  user-track page.

  Built, from clean, with the commands the book prints:
  - native Rust, mps2 bare-metal and mps2 FreeRTOS — `nros sync` + `nros build`,
    then the same three images again through
    `cargo build --manifest-path <leaf>/Cargo.toml --config
    <leaf>/build/<image>/nros-cargo.toml` from each leaf's PARENT;
  - `cargo run` through the generated runner boots the mps2 image in QEMU;
  - native C — `cmake -B build && cmake --build build`, and the same from a
    copy outside the checkout with NO `nros sync`;
  - a copy-out of the native Rust talker syncs and builds outside the checkout
    with only `NROS_REPO_DIR` on the command line.

  Three things the runs corrected that reading would not have, all now in the
  text:
  - the three ways a missing `nros sync` fails read very differently, and
    `cargo --config <path that does not exist>` is unrecognisable — cargo
    parses the argument as a dotted key and reports "key with no value,
    expected `=`". `workflow-by-platform.md` quotes all three.
  - **the one-line board switch is not true yet for a single-package Rust
    leaf** — issue 1305. It IS true for a C/C++ leaf and for a workspace, and
    the pages say why that differs.
  - **`nros build` does not resolve a single-package C/C++ leaf** — issue 1296.
    Those pages keep the leaf's own `cmake` pair.

  NOT done: `just probe bootstrap` was not run (/home at 99 %, and a container
  that clones + bootstraps rustup + builds the runtime would risk filling it).
  Both tracks were validated by extraction instead — the quickstart track
  renders steps 10/20/40, the zenoh track 10/20/30 — and
  `verify-zenoh-interop.sh` was fixed, because it still started the talker with
  `cargo run` after step 30 stopped teaching it.

  Also fixed here, because W4b and W5 were written in parallel and their code
  met for the first time on this branch: `LeafSystem::is_fallback` and
  `board_from` (deleted by W5, still called by W4b), and two helpers spelled
  `write_leaf` with one name and two meanings. All three were invisible to
  `just setup-cli` and to every runtime build — only a `--lib` test target
  compiles them, which is what `check-build` is for.

## Acceptance

- A fresh clone, then `nros build <image>`, builds one image per board family —
  a BUILD, not a gate (#393). `nros sync` + plain `cargo build --config
  build/<image>/nros-cargo.toml` builds the same image.
  **MET for Rust (native, mps2 bare-metal, mps2 FreeRTOS, both roads, W7).
  NOT MET for C/C++: `nros build` does not resolve a single-package C/C++
  leaf — issue 1296. Their leaf `cmake` pair is green.**
- Switching a single-package example to another board is ONE line in
  `system.toml`; `git diff` afterwards shows that line only.
  **NOT MET for a Rust leaf — issue 1305.** Measured: one line changed,
  `nros sync` reported `done.`, `nros build` failed with
  `cannot find nros_board_<new_board> in the crate root`. A single-package
  leaf is its own entry, so it still names its board crate in
  `[dependencies]`; D6's generation reaches the generated workspace entry
  only. Everything else — the triple, the link group, the runner, the cross
  compiler — did follow the one line. Met for a C/C++ leaf (no board crate)
  and for a workspace (generated entry).
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
