# Phase 527 — Runner and command packages: kinds, host build, dispatch, release underlay

**Status (2026-11-05). IN PROGRESS — W3, W1, W2, W2b and W2c landed
(`packages/runners/` and `nros-extension`; the `runner` and `command` kinds;
the host build role; declared overrides; colcon's duplicate rule among
source roots, overrides in `nros.toml`), W2d (one `--base-paths` meaning,
#3088), W2e (a duplicate inside one root is refused too, #3113), W4
(command dispatch, #3116), W5 (the first command packages, #3130) and W6
(the release underlay, #3122); W7 (scaffolds and checks) in review (#3146).** Implements
[RFC-0119](../design/0119-cli-extension-packages.md) D1 to D7, D9 and D10.
The first-party runner packages of D8 are built in
[phase-528](phase-528-run-on-host-and-emulator.md) and
[phase-529](archived/phase-529-flash-monitor-bench.md), which depend on W1 to W3 here.
W4 depends on [phase-526](archived/phase-526-cli-two-tier-restructure.md) W2 for the
reserved-verb list.

---

## Why

A new way of running an image, or a new `nros` verb, can only be added today
by editing nros. RFC-0119 makes both a package that the workspace builds,
and makes nano-ros's own runners and commands packages of the same shape, so
the path a user's runner takes is the path ours takes.

## Work items

### W1 — The `runner` and `command` kinds

- Teach the one `package.xml` parser (`cargo-nano-ros/src/package_xml.rs`)
  and `provider_scan` the two kinds. RFC-0087 D3's single parser covers
  them; no second reader.
- Read `nros-runner.toml` and `nros-command.toml` per RFC-0119 D2, only for
  the provider actually selected (RFC-0087 D4).
- Extend `check-provider-announcements`: refuse a package mixing host and
  target kinds, a command named like a built-in or reserved verb, and a
  stated field that differs from its derived value.
- Re-check the Zephyr `module.yml` keys RFC-0119 cites from memory
  (`west-commands`, runner files), and correct the RFC's prior-art paragraph
  if they differ.

Acceptance:

- `nros dev ws providers` lists a fixture runner package and a fixture
  command package.
- `check-provider-announcements` has a self-test case for each refusal.

Status: **DONE** (2026-11-02).

- `package_xml.rs` names the two kinds and classifies every kind's build role
  (`BuildRole::{Host, Target}`, `build_role_of`). The parser stays one parser
  and the vocabulary stays open: an unclassified kind constrains nothing.
- `provider_scan` refuses a package mixing host and target kinds with a scan
  error, rather than listing a provider `nros build` could not build.
- `cargo-nano-ros/src/extension_descriptor.rs` reads `nros-runner.toml` and
  `nros-command.toml` only for the provider a resolution selects
  (`resolve_runner_in`, `resolve_command_in`). It validates verbs,
  placeholders and protocol against `nros-extension`'s vocabularies, and
  derives `bin` from the package's single Cargo bin target. A stated `bin`
  that differs from it is refused.
- `check-provider-announcements` gains the `runner` and `command` families,
  waived from the empty-set refusal by a self-retiring `BORN_EMPTY` row each.
  It also gains A6 (host/target mix), A7 (a command named like a built-in or
  reserved verb) and A8 (a stated `bin` that differs from the derived one),
  each with a self-test case. A7 reads
  `packages/cli/nros-cli-core/builtin-verbs.txt`, which a test in
  `nros_cli_core::builtin_verbs` holds equal to the clap tree; it lists the
  RFC-0118 D2 verbs as reserved until phase-526 W2 places them.
- Fixture packages live in
  `packages/cli/nros-cli-core/tests/fixtures/extension_packages/`.
  `nros-cli/tests/extension_providers.rs` lists and resolves them through
  `nros ws providers`, which phase-526 moves under `nros dev`.
- The Zephyr `module.yml` keys were checked against Zephyr's module schema and
  the west documentation. `module.yml` has `runners: [{file: …}]` and no
  `west-commands`; west extension commands are declared in the west manifest.
  RFC-0119's prior-art paragraph is corrected.

### W2 — The host build role

- `nros build` builds a `runner` or `command` package for the host triple
  into `<ws>/build/nros/host/<package>/`, with its own cargo target directory
  (issue 0616: one target directory per workspace root).
- An image gains a dependency edge on the package providing its resolved
  runner, so `--packages-up-to` and `nros run`'s stale-build step include it.
- `nros <verb>` for an unbuilt command package refuses and names
  `nros build --packages-select <package>`.

Acceptance:

- In a fixture workspace holding one image and one runner package,
  `nros build --packages-up-to <bringup>` builds both, the runner for the
  host triple.
- Touching the runner's source rebuilds only the runner.
- Touching the image's source does not rebuild the runner.

Status: **DONE** (2026-11-02, #3022).

- `builder::discover` classifies each workspace package's build role from the
  same walk (`provider_scan::scan_workspace_packages` now records providers),
  as `Discovered::host`. Image planning reads `without_host()`, so no
  generated cargo or CMake root lists a host tool. A dependency on a host tool
  is exempt from the selection's closure check.
- `builder::host_tool` owns both facts: where a host tool builds
  (`<ws>/build/nros/host/<package>/`; cargo with `--release --target <host
  triple>` and its own `--target-dir`, or the package's own `CMakeLists.txt`
  into `cmake/`) and where its executable lands (`executable`, which refuses an
  unbuilt one naming `nros build --packages-select <package>`). W4's dispatch
  and phase-528's `nros run` call `executable`; nothing calls it yet, so the
  refusal is held by a unit test.
- `cmd::build::plan_with_host_tools` is what `nros build` drives: the host
  tools first, then the images. `plan_builds` still returns images only,
  because `nros run` and `nros image-facts` ask about images. The image edge:
  each image's `run` and `flash` plans name a runner; when a workspace package
  announces that name and the provider search path resolves it to that
  package, the image's bringup package gains a `<depend>` on it before the
  selection is applied.
- Decisions: the host directory is fixed (`--build-base` does not move it,
  because dispatch reads it); `-- <native args>` and `--check` do not reach a
  host tool; with no selection every host tool builds; a selection keeping
  only host tools builds no image.
- `nros-cli-core/tests/host_build_role.rs` plans the acceptance workspace
  (one bringup with a `linux` image, one runner package providing `native`).
  Measured on a copy of `examples/templates/multi-node-workspace` plus a
  runner package, with real builds: `nros build --packages-up-to demo_bringup`
  built `acme_runner` (x86_64-unknown-linux-gnu, release) and then the image.
  Touching the runner's `main.rs` recompiled only `acme_runner`; touching
  `talker_pkg/src/lib.rs` recompiled `talker_pkg` and `linux_entry`, and the
  runner binary's mtime did not move.

### W2b — Declared overrides of provider packages

Sequenced before W6, which puts nano-ros's runners on the search path as the
underlay a user's runner can replace. Implements the amended RFC-0119 D4.

- Keep the precedence `provider_scan::resolve_unique` already has (a later
  root wins: nano-ros root, workspace, `package_paths`, `NROS_PACKAGE_PATH`),
  and rewrite D4, which said the opposite.
- `[overrides] <name> = "<package path>"` in `system.toml`, read by the one
  `SystemToml` parser, workspace-relative and path-pinned.
- At resolution (the W2 build edge, `run`/`flash` runner resolution, command
  dispatch): refuse an undeclared override of a `runner` or `command`, warn
  for other kinds, refuse an override that shares no protocol version with
  the package it replaces, error on a stale entry, warn on an unused one. A
  scan or a listing never refuses.
- `--allow-overriding <name>…` on the verbs that resolve providers.
- `nros dev ws providers` and `nros doctor` list every override.

Acceptance: one test per rule: undeclared runner refused at resolution while
a scan is silent; declared path wins; a declaration pinned to another path
does not approve; `--allow-overriding` approves; a non-runner kind warns; a
protocol mismatch is refused; a stale declaration errors; an unused one warns;
the W2 build edge follows the declaration.

Status: **DONE** (2026-11-03, #3050).

- **One lookup:** `cargo_nano_ros::provider_overrides::resolve_provider(scan,
  kind, name, policy) -> Result<PolicyResolution, PolicyResolveError>`
  applies precedence, `[overrides]`, `--allow-overriding`, the protocol check
  and the stale/unused rules. Every resolution goes through it: the W2 build
  edge, the rmw dispatch, and `extension_descriptor::resolve_runner_with` /
  `resolve_command_with` (it plus the descriptor read), which phase-528's
  runner lookup and W4's dispatch call. `report` is the non-refusing listing
  over the same classification. The old `resolve_runner_in` /
  `resolve_command_in` apply an empty policy, so with nothing declared an
  override is refused.
- `nros-cli-core/src/provider_overrides.rs` reads `[overrides]` from the
  workspace's `system.toml` (the root's, else every `src/*/system.toml`; one
  name pinned to two paths is refused) and renders the report both listings
  print. `SystemToml` gains the `overrides` table. (W2c moves the
  declarations to `nros.toml [workspace.overrides]`.)
- The W2 build edge is `cmd::build::runner_edges`, which resolves each image
  runner under the policy; `nros build --allow-overriding <name>…`.
- `nros dev ws providers` prints an override block (and an `overrides` array
  in `--json`); `--resolve kind:name` says what a resolution would do.
  `nros doctor` prints the block for the current workspace and counts a stale
  entry as a problem. `nros dev rmw dispatch` warns on an undeclared rmw
  override.
- Tests: `nros-cli-core/tests/provider_overrides.rs`, over the tracked
  fixture trees under `tests/fixtures/extension_packages/overrides/` (a
  `COLCON_IGNORE` keeps them out of the nano-ros tree's own scan).
- The runner side was phase-528 W4's, and landed with phase-529 W3 (the PR
  that added `nros flash` and `nros monitor`). `run_exec::locate_runner`, the
  one runner lookup of `nros run`, `nros flash` and `nros monitor`, calls
  `resolve_runner_with` with the workspace policy. `nros run` takes
  `--allow-overriding`. An undeclared override is refused with exit 3, and
  the policy's warnings print once. `nros run --dry-run --why` ends with
  the winning runner package and its `OverrideNote`. Command dispatch (W4)
  calls `resolve_command_with` the same way.

### W2c — Source roots are peers; overrides live in `nros.toml`

Two maintainer decisions on top of W2b (2026-11-03), recorded in RFC-0119 D4.

- **colcon's duplicate rule among source roots.** The workspace and its
  `package_paths` entries are source roots. The same `(kind, name)` in two of
  them is refused for every kind, naming both paths and the line to add,
  unless a declaration pins one (the pinned package wins wherever it sits) or
  `--allow-overriding <name>` lets the later root win. The nano-ros root stays
  the underlay (W2b's rules) and `NROS_PACKAGE_PATH` the overlay: it may
  shadow anything with a warning, and a runner or command it provides still
  needs a declaration.
- **Where declarations are read.** `nros.toml [workspace.overrides]`, beside
  `package_paths`. Only a workspace with no `nros.toml` (a single-package
  leaf) falls back to its root `system.toml [overrides]`. A
  `src/*/system.toml` is never read for overrides, and one carrying
  `[overrides]` is refused, so a vendored package cannot grant itself an
  override; `[overrides]` in both `nros.toml` and the root `system.toml` is
  refused, keeping `nros.toml`.
- **The leaf fallback is transitional** (from the RFC-0121 / phase-531
  session): it lives in one function, `leaf_system_toml_fallback`, so
  phase-531 W7 can delete it in one edit, and when it reads a non-empty
  table it prints one stderr note telling the user to move it to
  `nros.toml [workspace.overrides]`.

Acceptance: one test per rule — a src/`package_paths` duplicate refused; a
pin resolves it; `--allow-overriding` resolves it; two `package_paths`
entries refused; an env-root shadow warns; the underlay rule unchanged;
declarations from `nros.toml`; the leaf fallback; `src/*/system.toml`
`[overrides]` refused; both files refused; the fallback's note. W2b's
tests stay green.

Status: **DONE** (2026-11-03, #3075).

- Still ONE lookup: `resolve_provider` gains a private `pick` step that
  applies the source-root rule on top of `resolve_unique`'s order, and
  `report` uses the same step, so a listing and a resolution agree. The
  roots' tiers come from `ScanResult::root_origins`, filled by the new
  `provider_scan::scan_search_path` (and `ScanResult::with_origins` for an
  index read back from disk); every resolving caller (`nros build`'s runner
  edge, `nros dev ws providers`, `nros dev rmw dispatch`, `nros doctor`)
  scans that way. A scan with no origins applies W2b's rules alone.
- `candidates` treats one package reached through two roots (a
  `package_paths` entry inside the workspace) as one package.
- `--base-paths` kept the ordered rule here, because it replaced the whole
  path; W2d makes its entries source roots.
- `nros_config::workspace_overrides` reads the table; the phase 212.I
  `nros.toml` rejection now accepts `[workspace]` holding `package_paths`
  and/or `overrides`. Messages quote the file the policy read
  (`OverridesFile`), and `nros dev ws providers` (`overrides_file`,
  `overrides_read_from` in `--json`) and `nros doctor` say where the
  declarations came from; an unsettled duplicate is a `DUPLICATE` row and a
  doctor problem.
- Tests: `nros-cli-core/tests/provider_overrides.rs` (fixture
  `late/late_acme` added) and `nros_config`'s
  `workspace_overrides_do_not_trip_the_212_rejection`.

### W2d — One meaning for `--base-paths`, colcon's

Maintainer decision (2026-11-04), recorded in RFC-0119 D4 and RFC-0117 D4:
respect colcon, fix the UX and security issues, one meaning in every verb.
colcon's `--base-paths` lists the SOURCE roots; they are scanned recursively
as peers, a duplicate package name is an error, order is irrelevant, and an
overlapping or repeated path counts once. Underlays are separate
(`AMENT_PREFIX_PATH`). Before this, `nros build` and `nros list` narrowed
discovery to the base paths (and never read `package_paths`), while `nros
sync --base-paths` replaced the whole provider search path, underlay
included; and `discover::select` kept the LAST of two same-named packages,
silently.

- One flag struct, `cmd::build::BasePathsArgs`, flattened into `nros build`
  and `nros list` (through `SelectionArgs`), `nros sync`, `nros dev ws
  providers`, `nros flash`, `nros monitor`, and `nros run` (through
  `ExecArgs`). Relative to the workspace; default `.` plus `package_paths`.
- One root-list function: `provider_scan::source_roots(workspace,
  package_paths, base_paths)`, read through `cmd::ws::source_roots`. Build
  discovery (`discover::discover_in` now takes its `SearchPath`) and the
  provider search path (`provider_scan::search_path_over`, which
  `build_search_path` now is over the default roots) both use it. The flag
  replaces the source roots only; the nano-ros underlay and
  `NROS_PACKAGE_PATH` stay, with W2b/W2c's precedence, and a `--base-paths`
  root is a source root (`RootOrigin::is_source`). `SearchPath::from_base_paths`,
  the whole-path replacement, is gone.
- `nros run`/`flash`/`monitor` pass the flag to the planning and the
  implicit build through one `run_plan::Selection::build_args`, and to the
  runner lookup: `run_exec::locate_runner` takes it, and now scans with root
  origins, so the source-root duplicate rule applies there as it does at the
  build edge.
- A package name in two directories is an error from discovery that names
  both, the root each came from (origin and entry as typed, and whether it is
  outside the workspace) and the fixes. A pin in `nros.toml
  [workspace.overrides]` naming one of the two directories, or
  `--allow-overriding <name>` (the later root wins), settles it, as for a
  provider. `select` refuses a duplicate in a hand-built `Discovered` rather
  than keep the last.
- A missing `--base-paths` entry is an error; an existing root holding no
  package, and a `package_paths` entry that is not a directory, are warnings.
  Repeated and nested base paths count once (the outer root stays).
- Visibility: `nros dev ws providers` prints each source root outside the
  workspace (and `outside_workspace` in `--json`), `nros doctor` lists them,
  `nros run --dry-run --why` prints the source roots when any is not the
  workspace itself, and `providers.json` records each root's origin
  (`root_origins`, beside `roots`; not part of the validity check).
- Tests: `nros-cli-core/tests/base_paths_one_meaning.rs` (every verb parses
  the flag to the same list and `nros run`'s implicit build receives it;
  discovery and the search path share the list; the underlay's `native`
  runner still resolves; an out-of-workspace root shows in the listing and
  `--why`; missing errors, empty warns; `nros build` refuses a duplicate
  across two base paths), `discover.rs` unit tests (duplicate across roots
  and between the workspace and `package_paths`, settled by a pin or the
  flag, overlap dedup, empty warning, `select`'s refusal), `provider_scan`
  unit tests (`source_roots`, `search_path_over`, the index's origins) and
  `ws.rs`'s `sync_base_paths_keep_the_underlay`.

Measured on `examples/workspaces/mixed`, warm (`nros sync <ws>` after one
warm-up), 5 runs each, median:

| binary | `nros sync <ws>` | `nros sync <ws> --base-paths <ws>/src` |
| --- | --- | --- |
| before (base `3519b10b2b`; the flag replaced the whole path) | 0.188 s | 0.186 s |
| after (the flag replaces the source roots; the underlay stays) | 0.174 s | 0.182 s |

Dropping the old underlay-replacing behaviour cost nothing measurable: the
`--base-paths` sync is 0.182 s against 0.186 s before, inside the run-to-run
spread (0.171 to 0.190 s across all four columns), and it is within 8 ms
(about 5 %) of the default sync it now matches. That agrees with issue 0646,
which put the underlay walk at about 0.02 s of a 0.194 s sync. No in-tree
caller uses `nros sync --base-paths` (`scripts/regenerate-bindings.sh` passes
`--no-provider-index`). Neither threshold (5 % over the old `--base-paths`
time, or 50 ms) is crossed, so **no underlay-scan cache was added**.

Deviations from the brief, each with its reason:

- **Two copies of one package inside ONE source root warned instead of
  erroring** in #3088, to keep `examples/templates/ament-workspace-port`'s
  two board roots (both `ament_workspace_port`) discoverable. Reversed by
  W2e below: the maintainer chose colcon's rule.
- **`nros sync`'s own package walk is unchanged.** It reads `src/<pkg>/`
  one level deep, a different walk from build discovery; on `sync` the flag
  sets the source roots of the provider index it writes, which is what
  `provider_roots_for_sync` did before. Making sync's package walk follow the
  source roots belongs with phase-530's `nros configure` and is not done here.
- **A `--base-paths` entry inside the nano-ros tree** is dropped from the
  provider search path, as a workspace nested in the tree already was: root
  0 walks it, so `--base-paths .` in an in-tree workspace is the default path.
  Build discovery still walks it as a source root.
- **A pin settles a discovery duplicate by path**: any `[workspace.overrides]`
  entry naming one of the two directories keeps it. Declarations are keyed by
  provider name, and a plain package has none, so matching on the key would
  leave a non-provider duplicate with no pin to use.
- **`run_exec::locate_runner` now scans with root origins**
  (`scan_search_path` rather than `scan_roots`), so the W2c source-root
  duplicate rule reaches `nros run`/`flash`/`monitor` as it reaches the build
  edge. Before, a runner duplicated across two source roots resolved by
  order there.
- **The default build discovery now reads `package_paths`**, as the brief
  asks. No tracked `nros.toml` declares any, so no in-tree workspace changes.

Status: **DONE** (2026-11-04, #3088).

### W2e — A duplicate inside one source root is refused too

The maintainer's call on W2d's first deviation: colcon's rule, everywhere.
colcon checks the whole discovered set and refuses any repeated name; which
base path a copy came from makes no difference.

- `discover::settle_duplicates` refuses two copies of one name whether they
  sit in one source root or in two. The in-root warning, and the deferred
  refusal in `select`, are gone; `select` keeps its check only as a backstop
  for a hand-built `Discovered`.
- The error names every fix, the ignore marker first among the cheap ones:
  rename one package, put an empty `COLCON_IGNORE` (or `AMENT_IGNORE`,
  `NROS_IGNORE`) in the copy that is not meant to build, drop a root, or pin
  a provider. Ignore-marked directories were already never walked, so a
  backup or vendored copy never reaches the check.
- The template's two board roots get their own package names,
  `ament_workspace_port_freertos` and `ament_workspace_port_zephyr` (the
  `project()` names and the `[system] name` are unchanged; nothing keys on
  the package name). A copy of the template is now a workspace colcon would
  also accept.

Why not keep the narrower rule: a duplicate that is quiet until something
names it is how a stray or planted `package.xml` gets in, and a rule that
depends on how the roots are split is one nobody can predict. The cost — a
user who keeps two same-named copies in one root gets an error — is one
`touch COLCON_IGNORE`, the same as colcon asks.

Status: **DONE, in review** (2026-11-04).

### W3 — `packages/runners/` and `nros-extension`

- Create `packages/runners/` as its own cargo workspace with its own lock, and
  add `runners` to the `packages/{…}/` lists in CLAUDE.md and ARCHITECTURE
  §1 (`check-package-directories`).
- Write `nros-extension` there:
  - the RunPlan types (RFC-0118 D3) and their JSON form;
  - the `describe` builder;
  - the console pump and the `nros-runner: link` line;
  - USB VID:PID and port lookup;
  - the exit codes.
- The core CLI depends on it by path, and `python3 scripts/gen-cli-source-dirs.py`
  records the new directory.
- `just setup-cli` builds the in-tree runner packages beside the CLI.
- Prepare `nros-extension` for crates.io publication (metadata, README,
  semver policy tied to `protocol`), without publishing yet.

Status: **DONE** (2026-11-01). `packages/runners/` is a cargo workspace with a
tracked lock (a `[rust] locked_roots` entry, seeded from the pin set with
`nros ws seed-lock`, then `just lock-update "" "" packages/runners`), excluded
from the root workspace. `nros-extension` holds `protocol` (versions and
negotiation), `plan` (the RunPlan with its JSON form, placeholder lookup and
command-template expansion that refuses an unknown placeholder), `describe`
(the reply and its builder; `protocol` reads as a number or an array),
`invocation` (a runner's own argv), `console` (`pump`, `report_link`,
`scan_stderr`), `usb` (VID:PID, a sysfs and `/dev/serial/by-id` walk, and a
lookup that answers found, not found or ambiguous) and `exit` (0/1/2, plus 3
for nros only). `nros-cli-core` depends on it by path and re-exports it;
`cli-source-dirs.txt` records the directory. `just setup-cli` builds the
runners workspace before its own skip check, since the CLI stamp does not
cover runner binaries; no runner package exists yet, so today it builds the
library only. `check cli-tests` runs the workspace's tests and clippy. Crate
version 0.1.0 tracks protocol 1 (README "Versioning"); `cargo publish
--dry-run` passes, and the crate carries no `publish = false`, so the dry run
checks rather than refuses.

Acceptance:

- `just setup-cli` produces the runner binaries.
- `check-cli-source-dirs` and `check-package-directories` are green.
- `nros-extension` passes `cargo publish --dry-run`.

### W4 — Command dispatch

- In the toolchain's `nros` (not the launcher, RFC-0097 D4): a verb with no
  built-in match looks up an announced `command`, sets `NROS_WS_ROOT`,
  `NROS_TOOLCHAIN` and `NROS_CLI`, and executes its `bin` by absolute path
  with the arguments unchanged. `tier = "dev"` places it under `nros dev`.
- `nros help --all` lists extension verbs with their summary and package.
- Reserved verbs from phase-526 W2 cannot be provided.

Acceptance:

- A fixture command package's verb runs, receives its arguments unchanged
  and the three variables, and its exit status passes through.
- A fixture command named `build` is refused at discovery.

Status: **DONE** (2026-11-04, #3116).

- `nros-cli-core/src/cmd/extension.rs` holds dispatch. `nros-cli`'s `main`
  calls it only when clap reports an unknown subcommand, so a built-in always
  wins; when no package announces the verb, clap's own error stands. The
  provider is `resolve_command_with`'s answer over
  `cmd::ws::provider_search_path_for` and the workspace's override policy,
  the same call every other resolution makes; nothing here scans or orders.
- The executable ladder moved out of `run_exec::locate_runner` into
  `builder::host_tool::locate_built`, which `locate_runner` and dispatch both
  call: the workspace's host build, a released toolchain's `bin/`, the
  checkout's `packages/runners` build, a prebuilt package's `bin/`.
- Every unknown verb reaches the scan, so the current directory's packages
  are walked only when it looks like a workspace (`extension::is_workspace`:
  an `nros.toml`, `system.toml` or `package.xml`, or a `src/` directory);
  elsewhere `cmd::ws::provider_search_path_without_sources` scans the
  underlay and `NROS_PACKAGE_PATH` only. Measured before the guard, on this
  host under load: a provider scan rooted at `~/repos` took 1 min 47 s, which
  every typo there would have paid.
- `locate_built` treats a package of the nano-ros tree reached from a
  directory holding the whole tree (`nros run` from the checkout's root) as
  the checkout's build. Before, `run_exec::locate_runner` looked for such a
  runner under `<checkout>/build/nros/host/` and refused it as unbuilt.
- The command gets `NROS_WS_ROOT` (the canonical current directory),
  `NROS_TOOLCHAIN` (this nros's version) and `NROS_CLI` (its canonical
  path), and its arguments byte for byte. On Unix nros `exec`s it, so the
  exit status, and a signal, pass through unchanged.
- A refusal (not built, override refused, wrong tier, a built-in name)
  prints why and exits 3. A `tier = "dev"` command reached as `nros <verb>`
  is refused naming `nros dev <verb>`, and the other way round.
- The built-in and reserved names are `builtin_verbs::taken`, read from the
  live clap tree by the same rule as `check-provider-announcements` A7
  (user: every top-level verb and the reserved ones; dev: those plus `nros
  dev`'s). `nros help --all` ends with an "Extension commands (from
  packages)" section (spelling, summary, package) and prints one warning per
  refused command, `build` included.
- Tests: `nros-cli/tests/command_dispatch.rs` over the new fixture workspace
  `tests/fixtures/extension_packages/dispatch/ws` (a user-tier and a
  dev-tier command package; the `build` one is written into the test's
  tempdir, since A7 refuses any tracked `package.xml` announcing a built-in
  verb, fixtures included): arguments unchanged (including `--help`
  and an empty one), the three variables, exit 7 passed through, the tier
  rule, the unbuilt refusal, the `build` refusal with `nros build` still the
  built-in, the help listing, and an unknown verb keeping clap's error. The
  "built" command is a shell script placed where `nros build` puts a CMake
  host tool, so the tests compile nothing; W2's tests cover the build.
- `tests/fixtures/extension_packages/` gains a `COLCON_IGNORE`. Without it
  the nano-ros root's scan found W1's `fixture_command`, and `nros help
  --all` in any workspace under a checkout would have listed `nros dev
  fixture-hello`. A root the caller names is scanned whatever markers it
  holds, so the tests that root a workspace in there are unchanged.

### W5 — The first command packages

- `nros_zenoh_tools` under `packages/rmw/zenoh/`, providing `router`:
  `nros router config` replaces `nros ws router-config`, which becomes a
  hidden alias that executes the package.
- `nros_px4_tools` under `packages/rmw/xrce/`, providing `px4`:
  `nros px4 msgs` replaces `nros generate-px4-msgs`, likewise aliased.
- The code moves out of `nros-cli-core`; the CLI keeps no copy.

Acceptance:

- Both old spellings give byte-identical output to the old built-ins on the
  same inputs, measured before the code moves.
- `nros-cli-core` has no router-config or PX4 module left.

Status: **DONE, in review** (2026-11-05, #3130).

- `packages/rmw/zenoh/nros_zenoh_tools` (`command/router`, bin
  `nros-zenoh-tools`, `nros router config`) and
  `packages/rmw/xrce/nros_px4_tools` (`command/px4`, bin `nros-px4-tools`,
  `nros px4 msgs`). Each is its own cargo workspace with a tracked lock
  (seeded with `nros dev lock seed`, refreshed with `just lock-update`; both
  are `[rust] locked_roots` rows, and the registry pin set did not move).
  Each `main` is the moved code with its messages unchanged, under a clap
  tree named `nros router` / `nros px4`. The PX4 generator itself stays in
  `rosidl-bindgen`, which the CLI shares.
- `nros-cli-core/src/cmd/generate_px4.rs` and `ws.rs`'s `run_router_config`,
  `RouterConfigArgs` and `RouterConfigFormat` are deleted. `nros
  generate-px4-msgs` and `nros ws router-config` are hidden verbs taking
  their arguments raw (`cmd::extension::AliasArgs`, help and version flags
  passed through) that call `cmd::extension::run_alias`: W4's lookup, with
  `msgs` / `config` prepended. `nros dev ws router-config` never existed, so
  no alias was added there. The help groups drop `generate-px4-msgs`; the
  `builtin-verbs.txt` list is unchanged, as the hidden verbs still exist.
- `release-nros.yml` "Build the CLI from source" builds both, so W6's
  `nros toolchain stage-packages` (which refuses a host-tool package with
  no built executable) stages them into `share/nros/packages/`.
- `just setup-cli` builds both packages beside the runners. The in-tree rung
  of `builder::host_tool::locate_built` now finds a checkout's host tool in
  the target directory of the cargo workspace it belongs to
  (`tree_target_dir`: the nearest `Cargo.toml` with `[workspace]` below the
  tree root), so `packages/runners/target/release` and
  `<command package>/target/release` are one rule.
- `check-provider-announcements` drops its `command` `BORN_EMPTY` row (the
  family now has two providers). The book's CLI reference documents `nros
  px4 msgs`, `nros router config` and extension commands. `just px4`,
  `scripts/dev/zenohd.sh`, the router artifact's header comment, the PX4
  READMEs, the bridge's CMake message and `build.rs` errors and
  `docs/reference/px4-xrce-companion.md` use the new spellings.
- Tests: `nros-cli/tests/command_aliases.rs` (each old spelling runs its
  package with the arguments unchanged, `--help` included, and is hidden
  from help; the new spellings reach the same packages), over fixture
  packages declared as `[workspace.overrides]`, so no real build is needed;
  `host_tool` unit tests for `tree_target_dir`.

Byte identity, measured: 14 invocations (router-config `override`/`json5`,
a missing rule, a malformed artifact, no `--model`; generate-px4-msgs rust,
rust with `--ros-edition jazzy --crate-version`, cpp `--topics`, cpp
humble, `--lang c`, rust `--topics`, no `--px4`, an unknown edition,
`PX4_AUTOPILOT_DIR`), on a synthetic PX4 tree (two `msg/`, one
`msg/versioned/`) and a hand-written router artifact. The baseline is the
CLI built from this branch's base before any code moved. After the move,
through the old spellings: every stdout, every exit code, and all 44
generated files (sha256) are identical.

**Deviation from the acceptance: stderr is NOT byte-identical.** It differs
in two ways only, both on error paths:
eyre's `Location:` line names the new source file
(`src/main.rs:NN` instead of `nros-cli-core/src/cmd/…`), and clap's usage
line for a missing `--model` reads `Usage: nros router config --model
<PATH>` instead of `… ws router-config --model <PATH>`. Neither can match
while the code lives in another binary, and the error text itself is
unchanged.

### W6 — The release underlay

- Amend the release job (`release-nros.yml`, RFC-0097) so the toolchain
  artifact carries `share/nros/packages/<package>/` for every first-party
  runner and command package: `package.xml`, descriptor, `bin/`.
- Discovery adds that directory as the nano-ros root when nros runs from a
  released toolchain (RFC-0119 D4); RFC-0087 D6's order is unchanged, and an
  override of a first-party package follows W2b.
- `just probe installed` (phase-447 A3) runs `nros run` on the native
  talker from an installed toolchain with no checkout.

Acceptance:

- The installed probe passes.
- A user package announcing `runner/qemu` replaces ours only when
  `nros.toml [workspace.overrides]` declares it (W2b, W2c); undeclared,
  `nros run` refuses it, and
  `nros doctor` reports it either way.

Status: **DONE, in review** (2026-11-05, #3122).

- **What ships.** `nros toolchain stage-packages <prefix>` (hidden, like
  `toolchain manifest`; `nros-cli-core/src/orchestration/release_packages.rs`)
  runs in `release-nros.yml`'s `Stage the prefix` step after
  `scripts/stage-sdk-root.sh`. It scans the staged SDK root with the one
  provider scan, takes every package whose announcements classify as a host
  kind (`kind_role`, so `runner` and `command` alike; no list anywhere), and
  writes `share/nros/packages/<package>/`: `package.xml` as is, the
  descriptor with `bin` stated, and `bin/<exe>` from the checkout's cargo
  target dir (`cargo metadata`). It re-scans what it wrote and resolves each
  provision through the descriptor reader before returning. No command
  package exists on main yet (W4/W5), so today it stages the six runners; a
  command package is staged by the same path, and the step refuses a host
  package whose executable was not built (so W5 adds its build line to `Build
  the CLI from source`).
- **Discovery.** `provider_scan::search_path_over`, the one root-list
  function, adds `share/nros/packages` as a second `NanoRosTree` root right
  after the nano-ros root when that root is a shipped SDK root
  (`nros_launcher::checkout::shipped_packages_beside`). The tiers and their
  order are unchanged; the W2b/W2c policy reads origins, so an override of a
  released runner follows exactly the rules of one in a checkout.
  `run_exec::locate_runner` lost its "`bin/` beside `nros`" rung (phase-525
  W4's): the released runner is found through its package's `bin/`, the
  rung that already existed. The release no longer copies `nros-runner-*`
  into `bin/`.
- **The probe.** `just probe installed` (track `first-project`) appends
  `scripts/probe/verify-installed-underlay.sh` after the chapter: the prebuilt
  runners are there and the SDK root carries no source copy; `nros run --why`
  names the released qemu runner; a user `runner/qemu` on `NROS_PACKAGE_PATH`
  is refused by `nros run` (exit 3) and reported by `nros doctor`; declared in
  the leaf's `system.toml [overrides]` it is used by a real `nros run
  --expect` (its executable wraps ours and leaves a mark) and still reported;
  then `nros setup linux --rmw zenoh`, `nros new native_talker --board linux
  --template talker`, `nros configure`, `nros run --expect 'Publishing:'`.
  `release-nros.yml`'s install check also runs the installed native runner's
  `describe` and lists it through `nros dev ws providers`.
- **Measured (2026-11-04):** `just probe installed` on this branch, the asset
  built by the workflow's own extracted steps on `ubuntu:22.04` (27 MB; six
  runners staged), installed into a clean `ros:humble` container: the
  chapter passed, then `the qemu runner resolves from
  /root/.nros/sdk/nros/0.0.0-probe/share/nros/packages`, `undeclared user
  runner/qemu refused (exit 3)`, `nros doctor reports the undeclared
  override`, `a declared user runner/qemu replaces ours` (its `nros run
  --expect` matched through the wrapper), and the native talker's `nros run`
  matched `Publishing: 'Hello World: 1'` after 1.3 s.
  After rebasing onto the phase-530 W1 merges (2026-11-05) the track stops
  earlier, in the chapter's own step 50, before any W6 check runs: the
  implicit configure refuses an unset `AMENT_PREFIX_PATH`
  ([issue 2275](../issues/2275-installed-probe-configure-needs-ament.md)).
  The branch touches neither the chapter nor configure.
- Tests: `release_packages` unit tests (a runner and a command staged, source
  removed, an announcement-free package kept, an unbuilt exe refused, `bin`
  stating), `provider_scan`'s
  `a_released_sdk_root_adds_its_prebuilt_packages_to_the_underlay`, the
  launcher's `a_shipped_sdk_root_finds_its_prebuilt_packages`, and
  `nros-cli-core/tests/release_underlay.rs` (the real qemu package staged;
  resolves from the underlay; an undeclared `my_qemu` refused and listed; a
  declared one wins, shadowing the prebuilt package, and listed).

Deviations:

- **The released nano-ros root is two directories**, `share/nano-ros` and
  `share/nros/packages`, not `share/nros/packages` alone. The SDK root has
  been root 0 of a released `nros` since phase-447 A2 (it carries the message,
  board and platform packages), so replacing it would lose `std_msgs`.
  RFC-0119 D4 now says so.
- **The release moves each runner package out of the SDK root.** The SDK root
  stages `packages/` whole, so without the move the underlay announced every
  runner twice. `packages/runners/nros-extension` stays (it announces
  nothing), beside a `Cargo.toml` whose member list now names missing
  directories; nothing builds that workspace from a release today, but W7's
  copy-out scaffold must not path-depend on it there (crates.io is the plan).
- **The native runner now sends the image's stderr to the console**
  (`nros_runner_native`, phase-528's package). The probe found that `nros
  run --expect 'Publishing:'` never matched the native talker: its
  `nros_log` lines are on stderr, and the runner protocol grades stdout only,
  so the run printed 880 `Publishing:` lines and timed out. The qemu runner
  already merges the two on a semihosting console for the same reason.
- **The probe stops the chapter's kept `nros run`** before its own runs:
  `probe-run.sh` records a `probe-keep` step's process group in
  `NROS_PROBE_KEPT_PGIDS`, because that run's router holds port 7447 and the
  first attempt failed with "the router exited (code 1)".

### W7 — Scaffolds and checks

- `nros new <name> --kind runner|command --lang rust|cpp` writes a package
  with `package.xml`, the descriptor, and a stub that implements `describe`
  and one verb on `nros-extension`.
- `nros dev runner check <name>` and `nros dev command check <name>`: build,
  `describe`, protocol check, and a dry-run plan.

Acceptance:

- A scaffolded runner, copied out of the tree, builds in a fresh workspace,
  passes `nros dev runner check`, and runs the native talker through
  `method = "<its name>"` at the image level.
- This is a `check-template-copy-out` row.

**Status: in review (#3146).** What landed:

- `nros new <name> --kind runner|command [--lang rust|cpp]`
  (`cmd/new_extension.rs`). It writes `package.xml` (build type
  `nros_cargo` or `nros_cmake`), the descriptor (`nros-runner.toml` with
  protocol 1, `verbs = ["run"]` and `needs = ["elf"]`, or `nros-command.toml`
  with tier `user`), a README and a stub. The Rust runner stub is built on
  `nros_extension::runner::main`: it answers `describe` and runs `elf` with
  the pass-through arguments. A test checks that the stub's answer agrees
  with its descriptor. The package lands in `src/<name>` when the current
  directory is a workspace with a `src/`, otherwise in `<name>`, and
  `--destination-directory` overrides both.
- `nros dev runner check <name>` and `nros dev command check <name>`
  (`cmd/extension_check.rs`). Each resolves the package through the same
  provider resolution as dispatch, builds it with `nros build
  --packages-select` (skip that with `--no-build`), and locates the built
  binary through `host_tool::locate_built`. The runner check calls
  `describe` for each verb in its descriptor and compares the verbs and needs.
  It then makes a dry-run plan, `nros run --dry-run --json`, which must name
  this runner and carry its needs. That step is reported NOT RUN when the
  workspace has no single image, or when its image selects another runner;
  in the second case it prints the `[image.<id>.run] method` row that would
  select this one. The command check refuses a built-in verb
  name, negotiates the descriptor's protocol, and runs `<bin> --help`. A
  non-zero exit there is a note, not a failure.
- **`[image.<id>.run] method | command`**: the acceptance's image-level
  `method` did not exist, because `[image.<id>.run]` held only `nic`. It now
  mirrors `[image.<id>.flash]`. The image rung sits above `[board.run]` in
  `run_plan::resolve_run`, `--why` names it, and `method` and `command` are
  alternatives: setting both is refused. `ImageRun::over` merges
  `[image_defaults.run]` key by key, so `method` and `command` travel
  together. Tested by `an_image_run_method_beats_the_boards` and
  `a_run_method_parses_and_travels_with_its_command`.
- `nros build`'s next steps print `nros run <image>` for an image whose
  runner is a runner package. They used to print "which this nros cannot
  launch yet", which stopped being true at phase-528 W3.
- A host tool that depends on `nros-extension` gets a `--config
  patch.crates-io.nros-extension.path=…` row on its build and on its lock
  configure, pointing at the checkout's `packages/runners/nros-extension`.
  The scaffold names the crate `version = "*"`, as every unpublished
  nano-ros crate is named. A tool that does not name the crate gets no row,
  so its build prints no "unused patch" warning. `nros-extension`'s manifest
  inlines its `workspace = true` fields, because it is now built from
  outside the root workspace.
- The `check-template-copy-out` row `scaffold:runner-package` copies
  `multi-node-workspace` out, scaffolds `acme_runner`, sets `[image.linux.run]
  method = "acme_runner"`, builds, runs `nros dev runner check`, and runs the
  talker through `nros run --expect Publishing`. It requires the runner's own
  `acme_runner: running` line in the output.

Deviations:

- **The C++ stubs do not use `nros-extension`.** It is a Rust crate with no
  C++ surface. The C++ runner stub prints the `describe` JSON itself and
  reads `elf` from the plan by hand. `nros dev runner check` holds the C++
  stub to the same protocol as the Rust one, so the two stay in step.
- **The command check runs `--help`, not `describe`.** RFC-0119's command
  protocol defines no `describe` verb, so the check negotiates the
  descriptor's protocol number and smoke-runs the binary.
- **The workspace test is temporary.** It uses `extension::is_workspace`
  until phase-531 W4's `new_workspace::enclosing_workspace` lands. The swap
  is one call.

## Acceptance

| Work item | Industrial | Field solutions | Education |
| --- | --- | --- | --- |
| W1–W2 | a vendor flasher is a reviewed package, built with the image | a customer's bench tool is one package | — |
| W4–W5 | — | domain verbs ship with their domain | — |
| W6 | an installed toolchain runs without a checkout | same | the lab image needs no checkout |
| W7 | — | a new runner starts from a scaffold | a course can ship its own runner |

## Out of scope

The runner packages themselves (phase-528, phase-529), publishing
`nros-extension` to crates.io (after protocol 1 has run through both of those
phases), and shell completions for extension verbs (RFC-0119 open
question 1).

## RFC-0120 (2026-11-03): what changes for this phase

[RFC-0120](../design/0120-nros-configure-one-resolver.md) adds `nros configure`. It covers what `nros sync` did, plus census, resolve and generated files. Every road calls it, and `nros build`/`nros run` call it implicitly when stale, like cargo. `nros sync` becomes its alias. A leaf that states no entities is sized from its census, and a host C/C++ compiler is required. Every host configure takes one pass, including a plain `west build`. The work is [phase-530](phase-530-nros-configure-single-pass.md). For this phase:

- **W2, the host build role.** Building a runner or command package is layer 2 (`nros build`), never `nros configure`. Configure invokes no target build tool and no `nros build` (RFC-0120 D1).
- **`nros run`'s stale-build step.** It is configure if stale, then the build including the runner package. A west or CMake hook calls configure only, so a plain `west build` never builds a runner package.
