# Phase 581 - One configuration registry: knobs, Kconfig, carriers and environment variables

**Status (2026-11-19). Active: design accepted by the user 2026-11-19; W0 done, W1 next.**
Implements [RFC-0140](../design/0140-configuration-registry.md) as widened on
2026-11-19: one registry with three row kinds (knob, statement, env) and many
contributors; one naming rule with a lint; the 88 settings that break it
renamed, their old spellings refused on every road; an open RMW set, so a
board or RMW package declares its own knobs and environment variables without
editing a central file; and the environment-variable rule of RFC-0140 Part B
(env over statement, warned once; IGNORE with a notice; REFUSE where unsafe).

Each item is one PR, claimed with `just claim phase-581-wN`. Every rename
stage is its own item and lands with its refusals. W10 to W15 (Part B)
depend only on W1 and may run beside W4 to W9.

## Why

RFC-0140's census on `origin/main` (`365ad8d9e6`): 112 registry rows, of
which 30 have an env stem that is not the mechanical form of their TOML key
and 14 a Kconfig symbol that is not their stem; 68 hand-written Zephyr
Kconfig settings outside the registry; 56 typed tenant fields restating the
registry; about 25 run-time variables with no registry; units spelled five
ways; two names that mean one thing at build time and another at run time;
and no way for a package to declare a row. The user ruled on 2026-11-19 that
environment variables and knobs share one registry and one naming scheme,
that rule-breaking names are renamed now with the old name refused, and that
board and RMW packages are self-contained.

## Work items

### W0 - Measure

- **Status:** DONE (2026-11-19)
- **Result:** M5 green and M6 refusing on Zephyr 3.7 and 4.4; `native_sim`
  has no environment; Humble ignores `ROS_NAMESPACE`; `SERVICE_TIMEOUT_MS`
  is core and `LET_BUFFER_SIZE` has no reader; `NROS_NODE_NAME` is read but
  does not rename a component node (filed as issue 2739). Data:
  [`data/phase-581-w0-measure.md`](data/phase-581-w0-measure.md); RFC-0140
  carries the results (D3, D4, D6, D7, D9, D10, Appendix A and B, Decisions
  3, 8, 11). Not measured: Jazzy (no install or image), the `freertos-posix`
  Rust road at run time (no in-tree image), the Cyclone, XRCE and bridge
  rows (no such image built). These are OPEN, not settled: W12 to W15 measure
  each before relying on it.
- The first draft's run-time measurements: a Linux image per road (Rust, C,
  C++) and a `native_sim` image, each variable of RFC-0140 Appendix B set,
  recording whether the process acted on it (Decisions 8 and 11).
  `ROS_NAMESPACE` against stock rcl/rclcpp on Humble and Jazzy (Decision 3).
- **M5:** on Zephyr 3.7 and 4.4, does a member added from an `osource`d
  file join `choice NROS_RMW_BACKEND` (declared with its prompt in
  `zephyr/Kconfig`), select, and refuse a conf assignment of another member?
- **M6:** what Zephyr's Kconfig does with a `prj.conf` assignment to an
  undefined symbol, on 3.7 and 4.4 (the hook refuses retired symbols itself
  either way; this records what a guest sees without the hook).
- The census of the `NROS_MAX_*` demand carriers' readers (cmake,
  `nros_cargo_build.cmake`, `NanoRosMessageBounds.cmake`,
  `NanoRosCodegenCore.cmake`, Rust build scripts), for W7.
- The owner of `SERVICE_TIMEOUT_MS` (read by `nros-c` and the zenoh
  backend); confirmation that `ENTRY_SPIN_MS` has no build-time reader; the
  unit of the seven rows whose help states none; the user-settable cmake
  `NROS_*` entries classified as row, resource or plumbing.
- **Acceptance:** RFC-0140's Appendix B carries a "measured" mark per row and
  any row the run contradicts is corrected in the same PR; M5 and M6 are
  recorded in RFC-0140 D6/D4 with the configure logs under
  `docs/roadmap/data/`; the census lists every reader by file and line;
  Appendix A is corrected where a measurement changes an owner or a unit.

### W1 - Registry schema: kinds, keys, owners, retired spellings

- **Status:** DONE (knob/statement/env rows with `key`, `transitional` and road-tagged `retired`; cmake and Kconfig views byte-identical, `knob_registry.rs` changed only by `retired_spellings` and the `ENTRY_SPIN_MS` row leaving for `[[env]]`; `nros_cli_core::knob_catalog` builds the merged catalog and configure writes `build/nros/knobs/catalog.toml`)
- `knobs.toml` gains the `owner` line, an `[owners]` table, `[[statement]]`
  and `[[env]]` tables, and per-row `key` and `retired`. `id`, `toml` and
  `kconfig` become derived; the generator refuses a row that states one. The
  32 `legacy` spellings and `RETIRED_ENV_VARS` move into `retired` (the
  latter as the `retired` entries of the `NROS_LOCATOR` and
  `NROS_SESSION_MODE` env rows) and `RETIRED_ENV_VARS` becomes a generated
  view. Every env variable of Appendix B gets a row with its disposition.
  `ENTRY_SPIN_MS` becomes an `env` row.
- No rename and no behaviour change: during this item `key` may differ from
  the current spellings, and the generator carries the current spellings as
  an explicit transitional column that W5 to W7 empty.
- **Acceptance:** the four committed views render byte-identical;
  `just check knob-registry` covers the new tables; a row whose `stated` is
  neither a statement key nor a knob key fails generation; the retired-name
  tests of phase-484 W8 and `check-retired-runtime-aliases` pass unchanged.
  The `nros fix` contract (RFC-0140 D4): a retired entry states `factor`
  (positive integer, new = old x factor, absent = 1) and optional
  `from_unit`, or `instead`, never both (the generator refuses both); the
  generated `KnobRow` carries `retired_spellings: &[RetiredSpelling]`; the
  loader exposes the in-memory build of the merged catalog (core plus the
  workspace's package rows, from the same package scan configure uses) and
  `retired_spelling(road, spelling) -> Option<(&Row, &RetiredEntry)>` over
  it, and configure writes `<build>/nros/knobs/catalog.toml` from that same
  call (the file is a cache for cmake and Kconfig, never the only source).

### W2 - Contributors: in-tree packages declare their own rows

- **Status:** DONE (59 knob and 7 env rows moved to six package `nros-knobs.toml` files; every committed view regenerates identical, the render sorts the merged set; C1-C4 and shadowing enforced by the Python reader and by `nros_cli_core::knob_catalog::Catalog::build`, the one Rust loader, with a test holding the provider scan's in-tree set equal to the generated `IN_TREE_PACKAGES`; configure records each package file as a stamp input)
- One loader (`nros_knob_catalog::load`) merges core `knobs.toml` with every
  `nros-knobs.toml` found beside a descriptor by the existing scans (board
  catalog, platform search path, RMW provider scan), enforcing RFC-0140 D2's
  owner derivation and C1 to C4. The zenoh, xrce, cyclonedds, freertos,
  threadx and zephyr rows move to their packages' `nros-knobs.toml`. The
  generator renders the committed views from the merged in-tree set.
- **Acceptance:** the committed views are byte-identical except row order
  (or identical, if the render sorts); a test plants two packages with one
  owner word and asserts C1's message names both files; a package row under
  another owner fails C2; a duplicated spelling fails C3; the same package in
  two roots is reported as shadowed, not refused.

### W3 - Out-of-tree contributors and per-build views

- **Status:** OPEN (landed: the loader split into `nros-knob-catalog` + `nros-knob-build::emit`, `NROS_KNOB_CATALOG` on the cargo, cmake and Zephyr Rust lanes, the ladder's catalog fallthrough, `registry.cmake`, `Kconfig.packages` + `osource`, `<owner>_env.h`, `nros knob list/show` with package and root, `nros knob render --kconfig`, the unlinked-package refusal, and both fixtures' knob and env row. Remains: the image's run-row table (its consumer is W10's startup sweep), `Knobs::registry` as the descriptors' parsed shape validated against the catalog, and two G6 runs on built images: the native_sim `.config` assertion in `out_of_tree_rmw_zephyr_e2e` and the board fixture's check in `check-out-of-tree-board-images` are written but were not run here. The acme linux image was built with `nros build` and printed `acme rmw: pool_slots = 4`)
- `nros configure` writes `<build>/nros/knobs/catalog.toml` and names it as
  `NROS_KNOB_CATALOG` (cargo `[env]` `relative = true`, cmake `-D`); the
  ladder `nros_zephyr_build::knob()` falls through to it;
  `nros_knob_build::emit()` for a package's build script;
  `<build>/nros/knobs/registry.cmake`; `<build>/nros/Kconfig.packages`,
  `osource`d by the module `Kconfig`; `<build>/nros/knobs/<owner>_env.h`;
  the image's run-row table generated into the entry; `nros knob list` and
  `show` over the merged catalog with package and root columns;
  `nros knob render --kconfig <package>` for a package that is its own Zephyr
  module (the guest case). `Knobs::registry` becomes the parsed shape for
  platform and board files, validated against the catalog.
- **Acceptance (RFC-0140 G6):** the out-of-tree RMW fixture
  (`fixtures/out-of-tree-rmw/`, `acme`) and the outside board fixture
  (phase-577 W8) each declare one knob and one env row with no edit outside
  the fixture; the image states the knob in `[image.<id>.knobs.acme]` and the
  package's build script reads the stated value (asserted from its output);
  the `native_sim` variant of the acme image sees the knob in
  `<build>/zephyr/.config`; `nros knob show acme.<name>` prints it with its
  package; an image stating a knob of a package it does not link is refused
  naming the owner.

### W4 - The naming rule and its lint

- **Status:** DONE (`nros_knob_catalog::naming` refuses a row that breaks N1 to N8 at load, in `Catalog::build` and a build script's `parse_own_file`, naming row and rule; the gate `just check config-names` on the fast line over the merged registry, the fixtures' package files and the hand `zephyr/Kconfig` symbols; rule data, unit allowlist (each term with its reason), the five `[[framework]]` Kconfig exemptions and the baseline in `packages/tooling/nros-knob-catalog/config-names.toml`, whose `[[case]]` corpus both implementations run; N8 reads `nros-sdk-index.toml` through the generated `RESOURCE_ENV_VARS`. The baseline is Appendix A's 88 entries (the `locator` statement's Kconfig symbol added to A.2 by the 2026-11-20 ruling), 13 of them judgements held by a presence probe; the gate fails on a new violation, a fixed entry and a count that is not `baseline_entries`)
- N1 to N8 enforced by the loader (so a downstream package with a bad name
  fails to load), and the gate `config-names` (RFC-0140 G1) running it over
  every in-tree contributor, with a baseline equal to RFC-0140 Appendix A
  that may only shrink. The unit allowlist (`mtu`, `baud`, `frames`) lives in
  the lint with a reason per term. The lint reads `nros-sdk-index.toml` so a
  setting cannot take a resource's variable (N8).
- **Acceptance:** the self-test plants one violation of each rule in a
  temporary catalog and the gate names the row and the rule; the baseline has
  88 entries and the gate fails if a new violation appears or if an entry is
  fixed but not removed.

### W5 - Renames, batch 1: structure and units (registry rows)

- **Status:** OPEN (the `nros fix` table test, which runs when phase-578 W4 lands)
- The Appendix A.1 rows whose reasons are S, U or F: the 15 three-segment
  keys flattened (`zenoh.wire.*`, `zenoh.limits.*`, `zenoh.tx.*`), the `_size`
  / `_len` / `_buf_size` / `_kb` / `_u64s` rows to `_bytes`, and the four
  platform heap and stack carriers folded into `memory.heap_bytes` and
  `memory.app_stack_bytes` (one carrier each; each platform's build converts
  at the one place that writes its RTOS configuration;
  `memory_env_key_for` deleted). `NROS_THREADX_BYTE_POOL_KB` (PR #4355) becomes
  `threadx.byte_pool_bytes` (x 1024), its derived default unchanged. Every tracked use updated (the tracked
  `[knobs.zenoh.tx]` table in one platform file and two
  `[image.<id>.knobs.zenoh.limits]` tables, the other image tables, C/C++ readers of the
  Kconfig symbols, fixture drivers).
- **Acceptance:** each old spelling is refused on each road its tag names,
  with the message naming the new key and, for a unit change, the factor
  (RFC-0140 G5, generated cases); `nros fix`'s table-driven test over every
  retired entry passes (each renamed `toml:`/`env:` spelling is rewritten,
  its value scaled by the factor and range-checked against the new row; a
  `kconfig:`/`cmake:` spelling is reported, not edited; an `instead` entry
  offers no edit); `just ci gate` green; the W4 baseline
  shrinks by exactly the renamed rows; one Zephyr, one FreeRTOS and one
  ThreadX image built before and after have identical `.config` values and
  identical `nm` sizes of the affected pools (measured, numbers in the PR).

### W6 - Renames, batch 2: owners, hand Kconfig, run-time and carriers

- **Status:** OPEN
- The Appendix A.1 rows whose only reason is P or K (the `params` family,
  the smoltcp rows to `net.*`, `zenoh.*` owner words); the 13 hand Kconfig
  symbols of A.2 (`CONFIG_NROS_ZENOH_LOCATOR` among them, retired as a
  `kconfig:` spelling of the `locator` statement); the 3 run-time renames of A.3 (the 6 retired `ZENOH_*` are
  W13's); the 6 build carriers of A.4, including the build-time
  `NROS_LOCATOR`/`NROS_DOMAIN_ID`/`NROS_NODE_NAME` reads moving to
  `NROS_ENTRY_*` (refusal for `NROS_DOMAIN_ID`, a build note for the two
  that stay run-time names).
- **Acceptance:** as W5 (refusals per road, `nros fix` table test, gate baseline shrinks, same
  `.config` before and after); a guest Zephyr project (no `system.toml`) with
  `CONFIG_NROS_CYCLONE_CONFIG_XML` in its `prj.conf` fails configure with the
  hook's message naming the new symbol; a FreeRTOS fixture build with
  `NROS_DOMAIN_ID` exported is refused naming `[system] domain_id`; a linux
  image built with `NROS_LOCATOR` exported builds, and runs with it.

### W7 - Renames, batch 3: the demand and pool split

- **Status:** OPEN (after W0's census)
- The six demand facts become core `entity.max_*` rows
  (`CONFIG_NROS_ENTITY_MAX_*`, `NROS_ENTITY_MAX_*`), replacing
  `CONFIG_NROS_MAX_*` and the `NROS_MAX_*` cmake carriers; the zenoh pools
  stay `zenoh.max_*` as carrier rows with no Kconfig symbol, derived from the
  demand through the existing floor (`_nros_c_array_pool_floor`). The `kconfig`
  column is deleted from the generator.
- **Acceptance:** `nros fix`'s table test over every retired entry passes
  (as W5); the `nros fix` table test runs when phase-578 W4 lands; the generator has no per-row Kconfig override; the
  `check-c-array-pool-floors` and `check-c-array-guard-probe` gates pass
  unchanged; a Zephyr image whose declared demand is 0 publishers still gets
  a pool of 1 (issue 1015's case), measured from `.config` and the generated
  header; the W4 baseline is empty and deleted.

### W8 - The open RMW set

- **Status:** OPEN (after W0's M5)
- `[rmw.zephyr] module = "<dir>"` in the descriptor; the hook adds the
  directory to `ZEPHYR_EXTRA_MODULES`; `Kconfig.packages` adds
  `NROS_RMW_<OWNER>` to the choice (W0's M5: green on 3.7 and 4.4); the
  hook refuses a conf member that disagrees with the image's `rmw`, because
  Kconfig takes the last assignment silently (M5);
  `ZEPHYR_MODULE_BACKENDS` derived from the in-tree members plus the scan;
  the in-tree members of the Zephyr and NuttX RMW choices rendered from the
  descriptors.
- **Acceptance:** a fixture RMW package that is its own Zephyr module
  (a stub transport is enough) configures a `native_sim` image with
  `rmw = "<it>"` and its module's CMake runs, with no edit outside the
  fixture; RFC-0071 D18's unit test compares the derived list with the
  rendered choice; `acme` (built over the zenoh member) still builds.

### W9 - Fold the hand-kept lists

- **Status:** OPEN
- The 68 hand settings of `zephyr/Kconfig` become rows (rendered through
  `kconfig_if` / `kconfig_attrs`); the 9 structural entries and the NuttX
  menus go into `config-exemptions.toml` with a reason each; the typed
  tenant structs of `platform_config.rs` are replaced by generated accessors
  over `Knobs::registry`; `Knobs::backends` is deleted; the 22 unregistered
  build-script names and the classified cmake entries become rows or
  exemptions. Gate `hand-kept-config-lists` (RFC-0140 G4).
- **Acceptance:** the gate's self-test plants a hand `config NROS_X` outside
  the block and a new typed field and names both; `zephyr/Kconfig` outside
  the generated blocks holds only the exempted entries; `nros knob list`
  shows the folded settings; a Zephyr `native_sim` and an `mps2_an385` image
  have identical `.config` before and after.

### W10 - The shared run-time reader, `nros-env`

- **Status:** OPEN (after W1)
- RFC-0140 D11: `get`, `resolve`, `notice_if_set`, `refuse_if_set`, the
  once-per-row bitset, the startup sweep over the image's generated run-row
  table and every `run:` retired spelling, the C ABI and `<nros/env.h>`;
  package rows from `$OUT_DIR/nros_knobs.rs`. `nros::env` keeps its ladder and
  `report`; issue 2714's two functions become per-row calls with unchanged
  output. Compiled for Linux and NuttX (Decision 9).
- **Acceptance:** issue 2714's `host_connect_facts` test passes unchanged; a
  unit test resolves one row twice in one process and sees one warning; a
  freestanding build of `<nros/env.h>` returns NULL without linking `getenv`;
  a NuttX QEMU image reads `ROS_DOMAIN_ID` set by NSH `set`.

### W11 - The gate `config-reads-through-registry`

- **Status:** OPEN (after W10)
- RFC-0140 G3: run-time reads outside `nros-env` and build-script reads
  (`rerun-if-env-changed=`, build-script `env::var`, `option_env!`) must name
  a registry row, a resource row or a cargo/toolchain variable; hand-written
  core code may not read a package-owned name. File set from `git ls-files`;
  ratchet baseline of today's bypasses.
- **Acceptance:** the self-test plants a bypassing `getenv` and an unknown
  `rerun-if-env-changed=NROS_X` and the gate names file and name; a core
  file reading `NROS_ZENOH_*` by hand fails.

### W12 - Batch: the identity rows

- **Status:** OPEN (after W10)
- `ROS_DOMAIN_ID`, `NROS_LOCATOR`, `NROS_RMW`, `NROS_NODE_NAME`,
  `NROS_NODE_NAMESPACE`, `NROS_SESSION_MODE` through `nros-env`, with the
  mismatch warning for the four issue 2714 did not cover; the
  `freertos-posix` Rust road reads them (Decision 11).
- Fixes issue 2739: `NROS_NODE_NAME` / `NROS_NODE_NAMESPACE` are read on
  every Linux road today and silently change nothing for a component or
  application-named node (measured in W0); W12 must make the read act or make
  the row IGNORE with a notice.
- **Acceptance:** per row, on Rust, C and C++ images: env unset uses the
  stated value; env set uses the env value; the warning appears exactly when
  both are set and differ, once.

### W13 - Batch: the middleware files and the `ZENOH_*` retirement

- **Status:** OPEN (after W10)
- `ZENOH_SESSION_CONFIG_URI`, `ZENOH_CONFIG_OVERRIDE` (RESPECT on both roads,
  lowered by the session-file reader), `CYCLONEDDS_URI` (compare and report
  only), the six invented `ZENOH_*` REFUSED naming their
  `ZENOH_CONFIG_OVERRIDE` key (Decision 4), `NROS_BRIDGE_<N>_*` against
  `[[domain]]`.
- **Acceptance:** a C/C++ Linux image acts on `ZENOH_CONFIG_OVERRIDE`
  (measured as in W0); each of the six refuses the boot naming its
  replacement key; each file row warns once when the image baked a file; the
  Cyclone warning says the two compose.

### W14 - Batch: IGNORE and REFUSE rows, logging

- **Status:** OPEN (after W10)
- The notices of RFC-0140 D9 (discovery range, `RMW_IMPLEMENTATION` when it
  maps to another backend, `ROS_NAMESPACE` per W0, `ROS_LOG_DIR`, `ROS_HOME`,
  the two ignored `RCUTILS_*`, loans, `ZENOH_ROUTER_CONFIG_URI`, non-enforced
  security); the refusal for enforced security; `RCUTILS_COLORIZED_OUTPUT`
  and `RCUTILS_CONSOLE_OUTPUT_FORMAT` in the Linux console sink on every road
  with it (Decision 5).
- **Acceptance:** per IGNORE row, set it, boot an image with two sessions,
  find the notice exactly once; `RMW_IMPLEMENTATION=rmw_zenoh_cpp` on a zenoh
  image prints nothing; per REFUSE row, the boot fails naming the row and its
  replacement; a C image's console output follows
  `RCUTILS_CONSOLE_OUTPUT_FORMAT='[{severity}] {message}'`.

### W15 - Batch: operational rows and the embedded build note

- **Status:** OPEN (after W10 and W6)
- `NROS_ENTRY_SPIN_MS` (seven readers to one), `NROS_ENTRY_SPIN_STEP_MS`,
  `NROS_ENTRY_EXPECT_MESSAGE_CALLBACKS`, census, profile, trace, dump,
  `NROS_XRCE_SERIAL_BAUD`, the Cyclone test controls, `RUST_LOG` (Rust road)
  through `nros-env`; the board descriptor's `process_env` fact and RFC-0140
  D10's build note.
- **Acceptance:** W11's baseline is empty and deleted; `NROS_ENTRY_SPIN_MS`
  has one reader; a CLI test configures an embedded image with
  `ROS_DOMAIN_ID` set and finds the note once, and with nothing set finds
  nothing.

### W16 - Docs

- **Status:** OPEN
- The book's configuration reference generated from the merged catalog (with
  phase-569 W2c), including the `env` rows and a "has an environment"
  column; a "Declaring knobs in your own package" section in the board- and
  RMW-package chapters; RFC-0036 rows for RFC-0140 D12; RFC-0109 D6a
  corrected if W0 says so; RFC-0103 D2b/D6, RFC-0049, RFC-0071 D18 and
  RFC-0133 carry an "Amended by RFC-0140" note; CLAUDE.md's knob-registry
  line names the contributor files and the naming rule.
- **Acceptance:** the book's docs gates pass; every row of the in-tree merged
  catalog appears in the generated page (checked by the generator).

## Acceptance for the phase

- One registry: every configuration name nano-ros defines or reads is a
  row, an exempted entry with a reason, or a resource row; G3 and G4 hold it.
- One naming rule: the W4 baseline is empty and deleted, and the loader
  refuses a new violation in-tree or downstream.
- Every renamed setting's old spelling is refused on its road, naming the
  new one (G5).
- An out-of-tree board package and an out-of-tree RMW package each declare a
  knob and an environment variable, and (W8) an RMW package brings its own
  Zephyr module, with no edit to a central file (G6).
- Every run-time variable has a disposition, a test and a row in the book; a
  user who sets one sees it take effect (and a warning if it disagrees with
  `system.toml`) or one line saying it does nothing and what to use instead.

## Related

- [RFC-0140](../design/0140-configuration-registry.md); RFC-0103 (D2b, D6), RFC-0049,
  RFC-0071 (D16, D18), RFC-0133 (phase-569 W2), RFC-0135 (phase-577),
  RFC-0045, RFC-0109.
- Issue 2714 and PR #4285; phase-484 W8 (the retired-spelling pattern).
