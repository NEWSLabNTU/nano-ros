# Phase 527 — Runner and command packages: kinds, host build, dispatch, release underlay

**Status (2026-11-01). IN PROGRESS — W3 landed (`packages/runners/` and
`nros-extension`); W1 next, then W2.** Implements
[RFC-0119](../design/0119-cli-extension-packages.md) D1 to D7, D9 and D10.
The first-party runner packages of D8 are built in
[phase-528](phase-528-run-on-host-and-emulator.md) and
[phase-529](phase-529-flash-monitor-bench.md), which depend on W1 to W3 here.
W4 depends on [phase-526](phase-526-cli-two-tier-restructure.md) W2 for the
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
  released toolchain (RFC-0119 D4); RFC-0087 D6's order and shadow report
  are unchanged.
- `just probe installed` (phase-447 A3) runs `nros run` on the native
  talker from an installed toolchain with no checkout.

Acceptance:

- The installed probe passes.
- A user package announcing `runner/qemu` is shadowed and reported by
  `nros doctor`.

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
