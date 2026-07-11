# Phase 288 — source-distribution bootstrap: one front door, no false prebuilt

Status: **Complete — 2026-07-11** (W1+W2 `f163eb81e`, W3+W4 docs follow-up) ·
Implements #171 decisions **D1 + D2** · Informs RFC-0003, RFC-0014 (setup /
provisioning) · Sibling of phase-287 (CMake consumption reshape, #171 D5).

> **Landed notes.** W1 went further than `packages/cli/install.sh`: the audit
> found THREE prebuilt surfaces — `install.sh` (archived nros-cli Releases,
> newest asset 0.3.7), `scripts/install-nros-prebuilt.sh` (this repo's
> Releases; no `nros-v*` release exists; no CI consumer), and the SDK index's
> `[tool.nros]` (would install the stale 0.3.6 into the store). All three
> deleted. `.github/workflows/release.yml` (tag-gated artifact builder,
> skip-safe, never produced a release) is now **orphaned infra** — kept
> pending a maintainer keep-or-delete call. W2: bare `scripts/bootstrap.sh`
> is the front door (rustup-on-demand → CLI submodule → cargo build →
> "next → nros setup"); verified in an `env -i` shell with no `just`/`nros`
> on PATH. W3+W4: `installation.md` gained the end-to-end flow + the
> "Pinning a version" section (tag ↔ `nros version` lockstep ↔ index pin).

> **Goal.** A new user obtains nano-ros exactly one way: pull the source at a
> pinned version, run a single **bootstrap script that builds the `nros` CLI
> from source** (no `just`, no prebuilt download), then `nros setup <board>`
> for the rest of the prereqs. Remove the competing/false path that advertises a
> prebuilt-binary install from a Releases repo that does not exist.

## Why (verified 2026-07-10/11, #171)

Two bootstrap stories exist and they disagree:

- `scripts/bootstrap.sh` — "Bootstrap nano-ros from a fresh checkout without
  requiring `just` first"; **builds** the CLI. This is the intended front door
  (#171 D1: a fresh env may not have `just`).
- `packages/cli/install.sh` (Phase 195.A) — `curl … | sh` that **downloads a
  prebuilt** `nros` from a `NEWSLabNTU/nros-cli` GitHub Releases repo
  (`nros-v0.2.0`). Per #171 the CLI is per-checkout build only; that published
  binary does not exist. It is a **false availability claim** (the 4th, after
  the three fixed in #171 priority 1).

D2 fixes the model: nano-ros is a **source distribution**. The runtime is
mixed-language (no_std Rust core + C/C++ FFI) and users target many
platform×arch combos, so a prebuilt library is infeasible and crates.io cannot
carry the C/C++ deps. Consumption: pull pinned source → bootstrap (builds CLI) →
`nros setup <board>` → the consumer project's manifest points at the nano-ros
**entry manifest** (CMake include into the checkout — phase-287; Cargo
`[patch.crates-io]` / the phase-287-W4 handle → checkout).

## Waves

### W1 — truth-fix `install.sh` (do first; #171 priority-1 grade)
- **Do:** either delete `packages/cli/install.sh` or rewrite it to the real
  route (clone at a pinned tag → `scripts/bootstrap.sh`). Remove every reference
  to a `NEWSLabNTU/nros-cli` prebuilt Release and the `curl … | sh` one-liner
  from docs/READMEs until such a Release actually ships.
- **Acceptance:** no doc or script advertises a prebuilt `nros` download;
  `git grep -i 'nros-cli/.*releases\|install.sh | sh'` surfaces nothing
  live-facing.

### W2 — one bootstrap front door
- **Do:** make `scripts/bootstrap.sh` the single entry: builds `nros` from the
  checkout with `cargo` (no `just` dependency), installs it to `~/.nros/bin`
  (or exports via `activate.sh`), and prints the next step (`nros setup
  <board>`). Demote `just setup-cli` to an internal recipe that calls the same
  build path (no user-facing duplication).
- **Do:** verify the bootstrap has no hidden `just`/`nros` prerequisite — a
  clean container with only `git` + a Rust toolchain must reach a working
  `nros`.
- **Acceptance:** from a fresh checkout with no `just` on PATH,
  `scripts/bootstrap.sh` yields a runnable `nros`; `just setup-cli` still works
  and shares the build path.

### W3 — the consumption flow, documented (user-facing)
- **Do:** one getting-started page (or a section) states the D2 model end to
  end: `git clone --branch <tag>` → `scripts/bootstrap.sh` → `nros setup
  <board>` → in your project, point the manifest at the nano-ros entry manifest
  (CMake `nano_ros_bootstrap()` from phase-287; Cargo the phase-287-W4 handle).
  No publishing/future-work talk (#171 D7).
- **Acceptance:** a reader following only that page, from a bare machine, builds
  and runs a talker against their own project layout.

### W4 — pin + version story
- **Do:** document how a consumer pins the source version (a git tag) and how it
  relates to `nros version` / the SDK index, so "pull at a specific version" is
  concrete, not folklore.
- **Acceptance:** the pinned-version instruction resolves to a real tag; `nros
  version` agrees with the checked-out tree.

## Non-goals

- crates.io / prebuilt libraries (#171 D2 rules them out).
- PlatformIO publish, ESP-IDF registry execution/CI (#171 D4 — future work);
  Arduino (#171 D3 — dropped).
- The CMake macro + example migration — phase-287.
- Restoring `find_package(NanoRos)` / `install()` (Phase 140).

## Acceptance (phase)

- Exactly one advertised way to get `nros`: pull source → bootstrap. No prebuilt
  claim anywhere.
- Fresh-machine walkthrough (clone → bootstrap → setup → build a talker) works
  from the documented steps alone.
- `just setup-cli` retained as an internal alias, not a second front door.

## Sequencing

W1 (truth-fix, cheap, unblocks trust) → W2 (front door) → W3 (docs, depends on
W2 + phase-287's `nano_ros_bootstrap()`) → W4 (pin/version). W3 references
phase-287; land phase-287 W1 first or stub the CMake line.
