# Phase 500 — the host board is `linux`

**Status (2026-10-14). W1 and W2 landed on their branches; W3–W5 open.** W1 drops
`native` as a board name: the host board descriptor names `["linux"]`, every
tracked selector / deploy token / cmake `DEPLOY` / sdk-index key writes
`linux`, `native` is refused with a hint naming `linux`, and
`check-host-platform-vocabulary` holds it. W2 renamed the lane, the `just`
module, the setup/build/test/doctor scope and the host test binaries to
`linux`, keeping `native` as a deprecated alias. Image ids and the
`examples/native/` directory still say `native` (W3–W4), and the deprecated
aliases are W5's to retire.

Implements the naming rule of
[RFC-0093](../design/0093-board-naming-states-reach.md) ("a board name states
the reach of its build") for the host board, which RFC-0093 §4 had left
"unchanged (CLAUDE.md owns these)". Feature axes: ARCHITECTURE §2, RFC-0005,
RFC-0006. Homes issue
[#1109](../issues/1109-board-native-vs-linux-naming-sweep.md), re-homed from
[phase-445](archived/phase-445-board-choice-generates-leaf-config.md).

## The ruling (maintainer, 2026-10-14) — authoritative

Definitions:

| word | means |
| --- | --- |
| **native** | the broad class of general-purpose hosts (Linux, macOS, Windows) |
| **posix** | POSIX-compliant systems |
| **linux** | Linux specifically |

These replace CLAUDE.md's "native = ROLE (host build), posix/linux = REACH"
wording, and with it issue 1109's 2026-10-13 rule ("a host image writes
`native` unless it authors a Linux-only fact"), which the maintainer rejected.

Decisions:

1. The board selector `native` is DROPPED. The host descriptor says
   `names = ["linux"]`; `board = "native"` is refused with a hint naming
   `linux`; every tracked image and scaffold writes `linux`. → **W1**
2. The lane / `just native` module / `setup native` scope / `native_*.rs` test
   names become `linux`, with `native` kept as a DEPRECATED alias for one
   cycle. → **W2**
3. Image ids `[image.native*]` become `linux*`. → **W3**
4. `examples/native/` becomes `examples/linux/`. → **W4**, the last rename.
5. After W1–W4, every alias that still maps `native` or `posix` to the host
   board or lane is retired, so each refuses with a hint naming `linux`. → **W5**

Defaults stated to the maintainer and not objected to:

- The platform `posix` KEEPS its name (`nros-platform-posix`, the
  `platform-posix` feature, `NROS_PLATFORM=posix`), but its package config
  carries no Linux facts: `ZENOH_LINUX` and `-lrt` are derived from the target.
- `[tiers.*.posix]` keeps its name — it is external vocabulary
  (ros-launch-manifest resolves `"posix" | "native"`, play_launch defaults
  `--target posix`). Its `core` key is the one Linux-only key (it is applied by
  `sched_setaffinity`).
- No false *BSD / macOS support claims in the docs.
- The five `#if defined(__APPLE__)` branches in `nros-platform-posix`'s
  `platform.c` are deleted (AGENTS.md bans them; macOS cannot build the port —
  no `timer_create`).

## The survey (2026-10-14), summarised

Measured, not assumed:

- **M1** — `nros-board-linux` is Linux-only. A probe crate calling exactly what
  `apply_tier_affinity` calls (`cpu_set_t`, `CPU_SET`, `sched_setaffinity`) plus
  `timer_create`, checked with `-Zbuild-std`: `x86_64-unknown-linux-gnu` OK;
  `aarch64-apple-darwin` 6 errors; **`x86_64-unknown-freebsd` 2 errors**
  (FreeBSD names the type `cpuset_t`); `x86_64-pc-windows-msvc` 7 errors. This
  corrected CLAUDE.md, the descriptor comment and issue 1109, which all listed
  freebsd as working, and falsified the book's "*BSD is a supported path".
- **M2/M3** — `nros-platform-posix`'s C sources are POSIX.1-2008 plus BSD
  sockets, and NuttX compiles three of them verbatim under `-Werror`. `posix`
  is the true word for the platform.
- **M5** — the posix *package config* carried Linux facts: `[build.zenoh]
  defines` had `ZENOH_LINUX`, `system_libs` had `rt`, the CMake target linked
  `rt` unconditionally, and `nros-platform-cffi` emitted `-lrt` unconditionally.
- **M7** — a live bug: `nros setup linux` failed (`unknown board 'linux'`),
  because `nros-sdk-index.toml` had `[board.native]` and `[board.posix]` and no
  `[board.linux]`. `board_key_table.rs` did not read the index (0196's reach gap).
- **M8** — a stale descriptor copy, `nros-cli-core/tests/fixtures/
  board-workspace/packages/boards/posix/nros-board.toml`, still named
  `["native", "posix"]`; the vocabulary gate excluded `tests/`.
- **M9** — the "native" lane is exactly the fixture rows whose coordinate
  starts with `linux,` (threadx-linux, native_sim and freertos-posix are not in
  it), so `linux` is the true lane word too (W2).

Scale (tracked files, all senses): `native` 2615 files, `posix` 1340, `linux`
1355; most `native` hits are other senses (Zephyr `native_sim`, RTOS-native
priorities, prose) and are out of scope.

## Work items

- [x] **W1 — the board name.** One PR.
  - Descriptor `packages/boards/linux/nros-board.toml` `names = ["linux"]` (and
    its `package.xml` provides). `nros_orchestration_ir::HOST_BOARD_KEYS =
    ["linux"]`; `BOARD_PATHS` and `nros_entry_lower::BOARD_KEYS` drop `native`;
    `BoardFamily::Native` → `BoardFamily::Linux` (serde/`as_str` `linux`).
  - ONE retired-key table, `nros_entry_lower::RETIRED_BOARD_KEYS =
    [("native", "linux")]`, and one refusal text, `retired_board_key_hint`,
    asked by every site that refuses an unknown board: the image resolver
    (`resolve_board_id`), `board_facts::unresolved_board` (build stages and leaf
    settings), the `UnknownBoard` display, the Rust entry pack, `nros::main!`,
    `nros check` (application `deploy`). CMake cannot read Rust, so
    `cmake/NanoRosHostBoard.cmake` carries the same table and text
    (`nros_refuse_retired_deploy`, `NROS_HOST_DEPLOY`), asked by
    `_nros_deploy_to_platform`, `nano_ros_entry` and `nano_ros_node_register`.
  - Every tracked selector rewritten: 160 `board = "linux"` in 98
    `system.toml` / `.case` files; 18 `[package.metadata.nros.*] deploy` lists;
    20 cmake `DEPLOY` arguments; the cmake deploy token the CLI derives
    (`PlatformKind::Posix.cmake_deploy()`) and the colcon plugin's keys. The
    build directory is `build/<platform>-<rmw>-<board>`, so `posix-*-native`
    became `posix-*-linux` (61 `examples/fixtures.toml` lines, the test
    locators, the book).
  - Scaffolds write `linux`: `nros new --workspace` (`new_system.rs`),
    `nros new --platform` (`scaffold.rs`).
  - `nros-sdk-index.toml`: `[board.native]` + `[board.posix]` → `[board.linux]`
    (fixes M7).
  - The stale test descriptor (M8) mirrors production, moved to `.../linux/`.
  - posix package config (M5): `ZENOH_LINUX` is a `defines_conditional` on
    `target_match = "linux"`; `rt` is linked only where the target is Linux
    (`CARGO_CFG_TARGET_OS` in `nros-platform-cffi/build.rs`,
    `CMAKE_SYSTEM_NAME` in the posix CMakeLists); `system_libs` lists
    `pthread` only.
  - `platform.c`'s five `__APPLE__` branches deleted (nothing else referenced
    them).
  - Docs: CLAUDE.md "Naming" rewritten to the ruling; the false *BSD/macOS
    claims removed (book `platform-model.md`, `introduction.md`,
    `native-host.md`, `installation.md` macOS steps, `nros-c/src/platform.rs`,
    the board README, the descriptor comment); `nros setup native` /
    `--platform native` in the book and scripts → `linux`.
  - **CLI aliases (W5 retires them).** `nros setup native` and
    `nros new --platform native|posix` keep working with a deprecation notice,
    mapped to `linux`, because RELEASED book pages (tags `nros-v0.5.0`,
    `nros-0.5.0-nros{1,2}`) tell users to type them. `--platform posix` predates
    this phase; `nros setup native` replaces the removed `[board.native]` row.
- [x] **W2 — the lane, module, scope and test names.** One commit.
  - **Lane.** `_NROS_LANES` `native` → `linux` (`lane=linux`);
    `RunScope::Native` → `RunScope::Linux` (`test_scope`/`build_lane` say
    `linux`); `NROS_TEST_SCOPE` / `NROS_FIXTURE_LANE` / `NROS_FIXTURE_SCOPE`
    values (`justfile` `_check-fixtures-stale`, `check-fixtures-stale.sh`);
    `scripts/test/lane-filter.sh linux`; the `fixture-lane.sh` helpers
    (`nros_lane_coords_file`, `nros_lane_unclaimed`, `nros_lane_build_lane`,
    `nros_lane_modules`, `nros_lane_run_is_narrowed`, the stamp-cover arm).
  - **Module.** `just/native.just` → `just/linux.just`, `mod linux`;
    `PlatformId::Linux.just_module()` = `linux`; `BuildChannel::NativeFixtures`
    → `LinuxFixtures` (`just linux build-fixtures`); every `native::<recipe>`
    dependency (`linux::format`, `linux::build-examples`, `linux::check`); the
    lane fan-out loops in `build-test-fixtures-leaves`; `nightly.yml`'s
    `elsewhere` map and comments.
  - **Scope.** `_NROS_SCOPE_PLATFORMS` `native` → `linux`
    (`just setup|doctor|build|test linux`), the `_test-scope` / `_doctor-scope`
    host arms, the `just setup` menu, `check-scope-namespace`'s self-test.
  - **Test binaries.** The nine `tests/native_*.rs` → `tests/linux_*.rs`
    (`linux_api`, `linux_async_roundtrip_e2e`, `linux_entry_boot`,
    `linux_example_executor_bound_node_e2e`, `linux_example_pubsub_e2e`,
    `linux_example_reqresp_e2e`, `linux_main_macro_forms`,
    `linux_main_macro_misuse`, `linux_orchestration_tiers`), with the
    `Cargo.toml` `[[test]]`, every `.config/nextest.toml` `binary(...)` filter,
    `.config/lane-admission/gate.txt`, `.config/fixture-require-baseline.txt`,
    `lane_scope.rs` and the per-file gate tables in the SAME commit. The
    module recipe `test-native-api` → `test-linux-api`.
  - **Workflows.** `host-tests.yml` (`just setup linux`), `live-peer.yml`
    (`just linux test-live-peer-regression …`), `nightly.yml`, `gate.yml`.
  - **W1 bridge deleted.** `DEPLOY_SCOPE = {"linux": "native"}` is gone from
    `cmd::setup`'s workspace report and from `check-board-vocabulary.py`: the
    host deploy token IS the scope again.
  - **The deprecated alias (W5 deletes each).** ONE table,
    `_NROS_RETIRED_HOST_TOKENS="native=linux"` in
    `scripts/build/fixture-lane.sh`, with one function
    `nros_retired_host_token` that prints `DEPRECATED: 'native' is now
    'linux' …` and returns `linux`. It is asked by `nros_lane_arg`
    (`lane=native`), `nros_scope_normalize` / `nros_scope_validate_all`
    (`just build|test|doctor native`), the `just setup` scope arm
    (`just setup native`), `lane-filter.sh`, and the build stamp reader (a
    pre-W2 `lane=native` stamp, quietly). `just` has no module alias, so the
    module's twin is one private root recipe, `native *ARGS`, that prints the
    same notice and forwards to `just linux …`. The test
    `lane_build_covers_run::the_retired_native_lane_resolves_to_linux_with_a_deprecation`
    pins the alias and goes with it.
  - Left alone, each for a stated reason: image ids (`[image.native*]`, W3);
    `examples/native/` and every path into it, including
    `lint-example-leaf.sh`'s directory test (W4); the C ABI
    `nros_board_native_run_*`; `posix` as the platform; the gate recipe
    `just check native-check` (a gate name in the `check` module, not the
    module or lane — renaming it touches the gate lists, so it waits for W5);
    the interop cell ids `native-<workload>-…` (`interop::CELLS` names, a
    coordinate vocabulary of their own); Zephyr `native_sim`, the `native`
    flash runner, RTOS-native priorities and prose; and the HISTORY series
    (`docs/roadmap/`, `docs/issues/`, `CHANGELOG.md`, audit reports), which
    record what was typed then.
- [ ] **W3 — image ids.** `[image.native*]` → `[image.linux*]` (229 headers in
  95 `system.toml`), `examples/fixtures.toml` `image = "native…"` rows, the
  derived `native_entry` package names, settings paths
  (`build/native/nros-cargo.toml`), and every test keyed on an id.
- [ ] **W4 — `examples/native/` → `examples/linux/`.** The sibling example dirs
  are named for their board (`mps2-an385-freertos`, `threadx-linux`, …). ~310
  moved files, ~112 code references (test locators, `just` recipes, workflows),
  ~388 doc references. Last, so W2/W3 land against stable paths.
- [ ] **W5 — retire the aliases.** Remove every alias that still maps `native`
  or `posix` to the host board or lane: the W2 deprecated `native`
  lane/module/scope alias, `nros new --platform native|posix`,
  `nros setup native`, and any sdk-index / board-lookup / fixture-coordinate
  spelling of `posix` or `native` meaning the Linux host board (e.g.
  `BoardCatalog::resolve_deploy`'s platform-kebab fallback, which answers
  `posix` with the host board). Afterwards each refuses with a hint naming
  `linux`. `posix` STAYS as the platform's name (`nros-platform-posix`,
  `platform-posix`, `NROS_PLATFORM=posix`, `[tiers.*.posix]`); only its use as
  an alias for the Linux host board or lane goes.

## Acceptance

- **W1**
  - `check-host-platform-vocabulary` (fast line) asserts: no descriptor
    (CLI test fixtures included) names `native` or `posix`; the host descriptor
    names exactly `["linux"]`; no tracked `system.toml` / `.case` selector, no
    `[package.metadata.nros.*] deploy` list and no cmake `DEPLOY` argument is
    `native`/`posix`; the sdk index has `[board.linux]` and neither
    `[board.native]` nor `[board.posix]`. Self-test on every run.
    **Mutation-proved**: `board = "native"` back in
    `examples/native/rust/talker/system.toml` → rc=1 naming the line; a
    `deploy = ["native"]` in a Cargo.toml → rc=1; `DEPLOY native` in a fixture
    CMakeLists → rc=1; `names = ["native", "linux"]` in the descriptor → rc=1
    (two findings); `[board.linux]` renamed back to `[board.native]` → rc=1;
    each restored → rc=0.
  - `board_key_table.rs::the_host_board_keys_are_one_set_everywhere` holds the
    descriptor names, `HOST_BOARD_KEYS`, `BOARD_KEYS`' Linux rows,
    `BOARD_PATHS`' `LinuxBoard` rows, the entry-sizing predicate AND the sdk
    index's posix-platform `[board.*]` keys to `{"linux"}`, and asserts every
    retired key is refused naming its replacement. **Mutation-proved**: the
    sdk-index rename above fails it (`left: {"native"}`).
  - Native images still build and link after the posix-config change (a
    zenoh-pico built without `ZENOH_LINUX` does not compile its `unix.h` arm),
    and the native e2e sample passes.
- **W2** — `lane=linux`, the renamed `linux` module (`just` + module name + recipe) and the `linux` setup scope work; the
  `native` spellings print a deprecation and do the same; `check-nextest-binary-
  filters` and `check-lane-contracts` green; a gate (mutation-proved) fails a
  tracked `lane=native` / `just native` use outside the alias definition.
  **Gate:** `check-host-platform-vocabulary` check 7 (fast line) — no tracked
  file outside the history series spells the old lane / module / scope / test
  binary (`lane=native`, `just [setup|build|test|doctor] native`,
  `mod native`, `native::<recipe>`, `NROS_TEST_SCOPE|NROS_FIXTURE_LANE|
  NROS_FIXTURE_SCOPE=native`, `lane-filter.sh native`, `--scope native`,
  `just/native.just`, `native_<renamed test>`), and `native` is in neither
  `_NROS_LANES` nor `_NROS_SCOPE_PLATFORMS`; an alias line carries the pragma
  `retired-host-token: alias`. Self-test on every run.
- **W3** — no tracked `[image.native*]`; fixtures and tests resolve the renamed
  ids; gate (mutation-proved) refuses a new `[image.native…]`.
- **W4** — no tracked path under `examples/native/`; every locator moved in the
  same commit; gate (mutation-proved) refuses the old directory.
- **W5** — a gate (mutation-proved) that no alias path remains: each retired
  spelling refuses with a hint naming `linux`; docs and book updated.

## Issues homed here

| issue | why it belongs here |
| --- | --- |
| [#1109](../issues/1109-board-native-vs-linux-naming-sweep.md) | `board = "native"` selectors; re-homed 2026-10-14 from phase-445. Partially done by W1; closes with W4 (W5 retires the aliases it leaves) |
