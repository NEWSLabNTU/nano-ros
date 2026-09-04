# Phase 201 — Custom-board provisioning (out-of-tree boards self-describe deps)

**Goal.** Let a user's own board crate declare its source/tool deps in its
`nros-board.toml` so `nros setup <custom-board>` provisions them out-of-tree —
without an entry in the maintainer-owned central `nros-sdk-index.toml`.

**Status (2026-09-04). SUPERSEDED — archived unstarted.** Deferred as proposed
2026-05-29 and never picked up; in the 97 days since, the architecture moved
underneath it and the question this phase exists to answer no longer has a place
to be asked.

**RFC-0087 deletes the lookup rather than extending it.** This phase's whole
shape is "on a central-index MISS, fall back to reading the board crate" — one
more branch in a resolver. RFC-0087's premise is that there is no resolver to
branch: *"a package is a directory containing `package.xml`, and nothing else
identifies one"*, and *"there is no builtin road"*. A user's board is found by
`provider_scan` exactly as `packages/boards/*` are, and its dependencies are its
`<depend>` entries. Nothing has to miss first.

Every work item has a successor, verified against `main` 2026-09-04:

| item | successor |
| --- | --- |
| **201.1** dep schema + resolver branch | RFC-0087 (`<depend>` + `provider_scan`); external trees become **vendor packages** reached by package name — phase-420 **W8**, which deletes the `[source.*]` row in the same commit |
| **201.2** `cargo_install` tool kind | RFC-0062 **`[prereq.*]`** — one key namespace over four providers, ordered `providers` list. (Note the name: nano-ros builds its own rosdep-shaped thing; rosdep itself is deliberately no longer consulted, and the fallback was deleted.) |
| **201.3** out-of-tree discovery / `--board-manifest` | phase-420 **W6** — `[workspace] package_paths` in `nros.toml` + `NROS_PACKAGE_PATH`, nano-ros tree first, shadowing **reported** by `nros ws packages` rather than silently won |
| **201.4** `nros new --board` scaffolder | **SHIPPED**, by phase-290 W4.b — `nros new board <name>` (`cmd/new.rs:36`). Not by this phase |
| **201.5** fresh-machine acceptance lane | phase-420 W6 / W8 acceptance |

**The premise is still literally true, which is why this needs saying out loud.**
`resolve_packages` (`packages/cli/nros-cli-core/src/cmd/setup.rs:1067`) still
bails with *"Add a `[board.<name>]` entry to `nros-sdk-index.toml`"*. Anyone
reading only the code would conclude this phase is still the answer. It is not:
the gap closes when boards stop being things the index knows about.

**And one narrow instance already shipped, in the direction this phase wanted.**
`nros setup board <name> --zephyr-workspace` reads the board's own provisioning
contract (`board.cmake`) out of the crate directory rather than the index
(phase-215.J.2, `setup.rs:429-467`). That is 201.1's shape, board-scoped and
Zephyr-only — evidence that the pull was real, not that this phase should be
revived.

*Original status, 2026-05-29:* Deferred. Design complete; implementation
parked. Pick up after the active Phase 200 line.

**Priority.** P3 — no nano-ros-internal board needs it; it's the enabler for
third-party / vendor boards built on nano-ros.

**Depends on.** Phase 195.C (`nros-board.toml` build-config descriptor), Phase 197
(`nros setup` is the single provisioning entrypoint; `[source]`/`[tool]`/`[gated]`
index kinds; builds consume nros-store tools), Phase 197.5 (nros-0.3.1 index schema
+ the deny-unknown-fields lesson — schema additions must be released).

**Design.** Full exploration + real-board survey + simulated walkthrough in
[`docs/design/0013-custom-board-provisioning.md`](../design/0013-custom-board-provisioning.md)
— **also marked Superseded (2026-09-04)**, and kept for the survey rather than
the mechanism: it is the evidence for what a third-party board actually needs,
which RFC-0087 inherits without restating.
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
