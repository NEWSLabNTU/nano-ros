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

### D3 — Out-of-tree projects (`nros new`) — the convention (decided 2026-06)

**Canonical: the same `nros ws sync` patch-block model as the in-tree examples
(D2).** A `nros new` project is scaffolded with crates.io-shaped requirements
that exist only to be patched — never a real crates.io version:

```toml
[dependencies]
nros = { version = "*", default-features = false, features = ["<rmw>", "platform-<plat>", "ros-humble"] }
<board-crate> = "*"
# msg crates (if any) are also "*" and emitted/redirected by `nros ws sync`.

# `nros ws sync` writes/refreshes the nros-managed [patch.crates-io] block here
# (path deps into the user's nano-ros checkout, NROS_REPO_DIR) — same delimited
# BEGIN/END block as the examples (RFC-0023).
```

The user runs, once, before the first build:

```sh
export NROS_REPO_DIR=/path/to/nano-ros      # their source checkout
eval "$(nros ws env)"                        # ROS / interface search path
nros ws sync                                 # codegen msgs + write [patch] block
cargo build                                  # plain cargo
```

**Rationale (D-Q1 resolution).** Chosen for **one convention everywhere**: a
msg-using project must run codegen anyway, and `nros ws sync` does codegen *and*
writes the patch block for both the generated msg crates and the `nros-*` runtime
crates in a single step (RFC-0023). A `git`-dep shape would leave msgs on a
second mechanism. Single mechanism, offline after sync, identical to the examples.

**Alternative (documented, opt-in): git deps** — `nros new --deps git` emits
`nros = { git = "https://github.com/NEWSLabNTU/nano-ros", package = "nros", … }`
(+ the board crate via `package=`, one repo fetch deduped). For zero-setup /
binary-only installs / CI quickstart / no-msg projects where no local checkout is
present. Msg-using projects on this path still run `nros generate-rust` and carry
the generated crates as local path deps.

**Constraint (both shapes):** **no `version = "0.1"` crates.io dependency** may
appear in a scaffolded manifest; `version = "*"` is only ever the patched
left-hand side. The emitted manifest + `nros new`'s post-create hint must
`cargo build` cleanly given a `nros setup`-provisioned host plus (canonical) a
nano-ros checkout or (alt) network for the git fetch.

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

- **D-Q1 — out-of-tree dep shape for `nros new`. RESOLVED 2026-06 → option 2**
  (`nros ws sync` patch-block) as canonical, **option 1** (git deps,
  `nros new --deps git`) as the documented opt-in alternative. See D3. Decided
  on the "one convention" + "msgs need the sync anyway" grounds; option 3
  (computed path dep) dropped as brittle + doesn't cover msgs. Remaining
  sub-questions below are minor and keep the RFC `Draft` until the 196.7
  implementation lands.
- **D-Q2 — version pinning without crates.io.** If git deps (option 1) are
  offered, what pins the version — a release tag, a branch, or a rev? Tie to the
  release cadence (is there a tagged source release, or is `main` the contract?).
- **D-Q3 — board + generated-msg crate coords. RESOLVED 2026-06.** `nros ws sync`
  now patches `nros-board-*` deps too: they resolve to the uniform
  `packages/boards/<name>` path (no static table entry — any current/future board
  crate works), alongside the `nros-*` runtime crates and generated msg crates.
  Verified: a `nros new … --platform freertos` scaffold's `nros` **and**
  `nros-board-mps2-an385-freertos` are both path-patched after sync, and the
  project resolves under plain `cargo`.
- **D-Q4 — does `nros new` itself run/scaffold the sync?** If option 2 is chosen,
  should `nros new` print the exact `eval "$(nros ws env)" && nros ws sync`
  follow-up, or attempt it when `NROS_REPO_DIR` is already set?

## Changelog

- 2026-06 — **D-Q1 resolved**: option 2 (`nros ws sync` patch-block) canonical,
  option 1 (`--deps git`) documented alternative; option 3 dropped. D3
  concretized with the scaffolded manifest + first-build steps.
- 2026-06 — initial Draft. Records the no-crates.io distribution policy (D1),
  the in-tree patch-block convention (D2, = RFC-0023), the out-of-tree
  constraint + the dep-shape open question (D3 / D-Q1), and the CLI-install
  correction (D4). Created as the design home for phase-196 item 196.7, which had
  no RFC.
