# Phase 527 — Runner and command packages: kinds, host build, dispatch, release underlay

**Status (2026-11-03). IN PROGRESS — W3, W1, W2 and W2b landed
(`packages/runners/` and `nros-extension`; the `runner` and `command` kinds;
the host build role; declared overrides); W4 next.** Implements
[RFC-0119](../design/0119-cli-extension-packages.md) D1 to D7, D9 and D10.
The first-party runner packages of D8 are built in
[phase-528](phase-528-run-on-host-and-emulator.md) and
[phase-529](phase-529-flash-monitor-bench.md), which depend on W1 to W3 here.
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
  print. `SystemToml` gains the `overrides` table.
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
  `[overrides]` declares it (W2b); undeclared, `nros run` refuses it, and
  `nros doctor` reports it either way.

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
