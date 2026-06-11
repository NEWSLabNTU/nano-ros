---
rfc: 0040
title: "Distribution model + `nros new` scaffolding dependency convention"
status: Draft
since: 2026-06
last-reviewed: 2026-06
implements-tracked-by: [phase-196]   # 196.7 — fix the dep convention for the source-release model
supersedes: []
superseded-by: null
---

# RFC-0040 — Distribution model + `nros new` scaffolding dependency convention

## Summary

nano-ros ships as a **source release + prebuilt host toolchains**, never as
crates published to crates.io. This RFC pins that distribution decision and
defines how a user project — both the in-tree examples and a fresh out-of-tree
project created by `nros new` — references the nano-ros crates so that a plain
`cargo build` resolves. It is the design-of-record for [phase-196] item 196.7,
which today scaffolds the wrong (`crates.io`) dependency shape.

## Motivation / problem

**Maintainer policy (confirmed 2026-06):** the nano-ros Rust crates (`nros`,
`nros-core`, `nros-rmw-*`, board crates, generated message crates) are **not and
will not be published to crates.io.** The project is consumed from its source
tree; host tools come from `nros setup` (RFC-0014).

This collides with two scaffolding/convention sites that still assume crates.io:

- `nros new` emits `nros = { version = "0.1", … }` and `<board> = { version =
  "0.1" }` (`packages/cli/cargo-nano-ros/src/scaffold.rs`). With nothing on
  crates.io, `cargo build` / even `cargo tree` fails to resolve
  (`failed to select … crates.io`).
- The README `cargo install` line for the CLI references the wrong org and mixes
  `--git` with `--path`.

The examples already solve their half: `nros ws sync` (RFC-0023) writes a
delimited `[patch.crates-io]` block redirecting `nros = "*"` (and the generated
message crates) to path deps into the in-tree source. The gap is the
**out-of-tree** project a user gets from `nros new` — it has no nano-ros source
tree beside it, so the path-patch trick has nothing to point at until the user
says where their nano-ros checkout lives.

There is no RFC home for this decision today: `nros new` scaffolding is gestured
at in RFC-0027 (a Stable *rationale* doc that predates Phase 222) and the layout
RFCs 0024/0025, but the source-release **dependency convention** is unwritten.

## Design

### D1 — Distribution model (decided)

- **No crates.io.** No `nros*` crate is ever published. The canonical artifact is
  the source tree (a git checkout / source release tarball) plus the `nros setup`
  host toolchains (RFC-0014).
- A user project references nano-ros crates by **path or git**, never by a
  crates.io version. `version = "x"` requirements only appear as the *left-hand
  side* of a `[patch.crates-io]` redirect (cargo requires the patched name to be
  a crates.io-shaped dependency), never as the resolved source.

### D2 — In-tree projects (examples) — unchanged, RFC-0023

In-tree examples keep the existing model: declare `nros = "*"` (+ generated msg
crates `"*"`) and let `nros ws sync` write the nros-managed `[patch.crates-io]`
block with path deps into the source tree. Already shipped; this RFC only records
it as the canonical in-tree shape. See RFC-0023 §`nros ws sync`.

### D3 — Out-of-tree projects (`nros new`) — the new convention

A `nros new` project lives outside the nano-ros tree. The scaffold must produce a
Cargo.toml that resolves against a user-pointed nano-ros source. The candidate
shapes are in **Open questions** (the genuinely-ambiguous part); the constraint
this RFC fixes is: **no `version = "0.1"` crates.io dependency may appear in a
scaffolded manifest.** Whatever shape D-Q1 selects, the emitted manifest +
`nros new`'s post-create hint must `cargo build` cleanly given only a
`nros setup`-provisioned host and a known nano-ros checkout.

### D4 — CLI install line (decided)

The README / book CLI-install instruction is the source-release form, e.g.
`cargo install --git https://github.com/NEWSLabNTU/nano-ros nros-cli` (or the
documented release-tarball `just setup-cli`), never a crates.io `cargo install
nros-cli` nor a contradictory `--git` + `--path`. Exact string tracked by 196.7.

### D5 — Scope / non-goals

- This RFC does not change the CLI verb surface (RFC-0003 §4: `nros` is
  provisioner + codegen + metadata; no `build`/`run`).
- It does not change codegen or `nros ws sync` mechanics (RFC-0023); it only
  states that the same patch-block mechanism is the in-tree dependency authority
  and decides the out-of-tree analogue.

## Alternatives considered

- **Publish to crates.io.** Rejected by maintainer policy (vendored forks of
  zenoh-pico / cyclonedds / libc and the tier-3 target matrix make a clean
  crates.io release impractical, and it is not a project goal).
- **`.cargo/config.toml [source] replacement`** instead of `[patch.crates-io]`.
  Rejected (RFC-0023): all-or-nothing, forces vendoring every transitive
  (non-ROS) crate, breaks mixed workspaces.

## Open questions

These are the **ambiguous parts** — the out-of-tree dependency shape is a real
maintainer choice, not yet locked. RFC stays `Draft` until D-Q1 is decided.

- **D-Q1 — canonical out-of-tree dep shape for `nros new`.** Three viable shapes,
  each with a trade:
  1. **`git` dep** — `nros = { git = "https://github.com/NEWSLabNTU/nano-ros",
     tag/rev = "…", default-features = false, features = […] }`. *Pro:*
     zero-setup, no local checkout, version-pinnable by rev/tag. *Con:* every
     build needs network + a git fetch; pins a rev the user must bump; the board
     + generated-msg crates also need git coords; diverges from the in-tree
     patch-block model (two conventions to maintain).
  2. **`nros = "*"` + `nros ws sync` patch block** (mirror the in-tree model) —
     the project declares `"*"` and the user runs `nros ws sync` with
     `NROS_REPO_DIR` pointing at their nano-ros checkout; sync writes the path
     `[patch.crates-io]` block. *Pro:* one convention everywhere; reuses shipped
     machinery; generated msg crates handled by the same sync. *Con:* requires a
     local checkout + an explicit `NROS_REPO_DIR` + a sync step before the first
     build (the scaffold hint must say so).
  3. **Direct `path` dep** computed at `nros new` time from `NROS_REPO_DIR`. *Pro:*
     simplest manifest, no sync. *Con:* brittle absolute/relative path; breaks if
     the project or the checkout moves; doesn't cover generated msg crates.
  **Leaning:** option 2 (consistency with the in-tree examples + RFC-0023, single
  mechanism), with option 1 offered as a documented zero-setup alternative for
  CI/quick-start. Needs maintainer confirmation before locking + implementing.
- **D-Q2 — version pinning without crates.io.** If git deps (option 1) are
  offered, what pins the version — a release tag, a branch, or a rev? Tie to the
  release cadence (is there a tagged source release, or is `main` the contract?).
- **D-Q3 — board + generated-msg crate coords.** The board crate
  (`nros-board-*`) and the codegen-emitted message crates need the same shape as
  `nros`; confirm the patch block / git coords cover them uniformly (the in-tree
  block already lists `nros-*` runtime crates + generated msg crates — D2).
- **D-Q4 — does `nros new` itself run/scaffold the sync?** If option 2 is chosen,
  should `nros new` print the exact `eval "$(nros ws env)" && nros ws sync`
  follow-up, or attempt it when `NROS_REPO_DIR` is already set?

## Changelog

- 2026-06 — initial Draft. Records the no-crates.io distribution policy (D1),
  the in-tree patch-block convention (D2, = RFC-0023), the out-of-tree
  constraint + the dep-shape open question (D3 / D-Q1), and the CLI-install
  correction (D4). Created as the design home for phase-196 item 196.7, which had
  no RFC.
