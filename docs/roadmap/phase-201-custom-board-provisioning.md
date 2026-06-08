# Phase 201 — Custom-board provisioning (out-of-tree boards self-describe deps)

**Goal.** Let a user's own board crate declare its source/tool deps in its
`nros-board.toml` so `nros setup <custom-board>` provisions them out-of-tree —
without an entry in the maintainer-owned central `nros-sdk-index.toml`.

**Status.** Deferred (proposed 2026-05-29). Design complete; implementation
parked. Pick up after the active Phase 200 line.

**Priority.** P3 — no nano-ros-internal board needs it; it's the enabler for
third-party / vendor boards built on nano-ros.

**Depends on.** Phase 195.C (`nros-board.toml` build-config descriptor), Phase 197
(`nros setup` is the single provisioning entrypoint; `[source]`/`[tool]`/`[gated]`
index kinds; builds consume nros-store tools), Phase 197.5 (nros-0.3.1 index schema
+ the deny-unknown-fields lesson — schema additions must be released).

**Design.** Full exploration + real-board survey + simulated walkthrough in
[`docs/design/0013-custom-board-provisioning.md`](../design/0013-custom-board-provisioning.md).
Builds on [`docs/design/0012-board-bsp-integration-architecture.md`](../design/0012-board-bsp-integration-architecture.md)
(the build-side / overlay-crate model).

---

## Overview

`nros setup <board>` resolves a board only from the central index `[board.*]`
(verified: nros 0.3.1 errors `unknown board … add a [board.*] entry`). Board crates
already self-describe **build config** (`cargo_config` + `${workspace}` → nros writes
the `.cargo/config.toml`) but **not deps**. The dep kinds a real board needs — git
trees, host tools, license-gated SDKs — already exist as nros `[source]`/`[tool]`/
`[gated]`; this phase lets a *board crate* carry them inline. Central index stays the
registry for nano-ros's own boards; user boards self-describe. A board id lives in
exactly one place (the Phase 197 no-drift invariant).

## Work Items

### 201.1 — Board-descriptor dep schema + resolver (nros-cli)
- [ ] Add `[[board.source]]` / `[[board.tool]]` / `[[board.gated]]` blocks to the
      `nros-board.toml` schema (same grammar as the index `[source]`/`[tool]`/`[gated]`).
- [ ] `nros setup <board>`: on a central-index miss, discover the board crate, read
      its `nros-board.toml`, and provision its declared deps + write the
      `cargo_config`. Index wins for nano-ros boards; crate descriptor for the rest.
- [ ] Version the descriptor / `#[serde(default)]` so an older `nros` degrades
      gracefully (the 197.5 deny-unknown-fields lesson). Cut the release that carries it.

**Files**: nros-cli `nros-cli-core` (board resolution / `SdkIndex`), the descriptor parser.

### 201.2 — `cargo_install` tool kind
- [ ] A `[[board.tool]]` `cargo_install = "<pkg>"` kind → `cargo install <pkg>` into
      the nros store (maker runners: `probe-rs-tools`, `elf2uf2-rs`, `picotool` where
      cargo-installable). Distinct from dist / `[tool.*.source]`.

**Files**: nros-cli tool provisioning.

### 201.3 — Out-of-tree board discovery
- [ ] `nros setup --board-manifest <path>` to point at an out-of-tree board crate,
      and/or board-name discovery across the workspace + a user search path. Document
      precedence + the "exactly one home" invariant.

**Files**: nros-cli `setup` cmd.

### 201.4 — `nros new --board` scaffolder
- [ ] Scaffold a board crate with a starter `nros-board.toml` (build config +
      `[[board.source]]`/`[[board.tool]]` stubs) — the entry point for a maker
      authoring a board.

**Files**: nros-cli `new` cmd + templates.

### 201.5 — Acceptance lane
- [ ] A fresh-machine lane mirroring the Phase 195 gate but for a **self-describing
      out-of-tree board**: install prebuilt nros → `nros setup <custom-board>` (reads
      the crate, provisions deps) → build → run. A sample board-crate fixture
      (`external/sim-board/my-rover-bsp` is the design's stand-in).

**Files**: `.github/workflows/`, a board-crate fixture.

## Acceptance
- A user's out-of-tree board crate with `[[board.source]]`/`[[board.tool]]` deps can
  `nros setup <board>` → provision + write build config, with **no** central-index
  entry — proven on a fresh machine.
- The four dep kinds (cargo / git / host-tool / gated) all reachable from a board crate.

## Notes
- **cargo vs nros deps.** A maker board's HAL is a *cargo* dep (`[dependencies]`,
  crates.io) — cargo fetches it; `[[board.source]]` is only for git/vendor trees cargo
  can't pull. Don't duplicate cargo's job.
- **Gated SDKs stay out of CI** (license) — `nros doctor` checks the env var, never
  downloads (today's `[gated.*]` behavior).
- The provisioning, config-writing, and store-tool-consumption mechanisms all exist
  (Phase 195/197) — this phase is the board-crate-as-dep-source wiring, one resolver
  branch + a schema, not a new mechanism.
