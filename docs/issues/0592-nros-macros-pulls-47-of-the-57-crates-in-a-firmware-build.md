---
id: 592
title: "a firmware build of `nros` compiles 57 crates; 47 of them exist only to run the `nros::main!` proc-macro"
status: open
type: tech-debt
area: build
related: [issue-0582, phase-262, phase-360, rfc-0032, rfc-0052]
---

## The number

```
cargo tree -e normal -p nros        --target thumbv7em-none-eabihf  ->  57 unique crates
cargo tree -e normal -p nros-macros --target thumbv7em-none-eabihf  ->  47 unique crates
```

Set difference — everything in the firmware build that is *not* under the
proc-macro:

```
nros  nros-node  nros-core (also under macros)  nros-params  nros-log
nros-platform  nros-platform-api  portable-atomic  portable-atomic-util
atomic-waker  paste
```

Eleven crates of actual embedded runtime. The other forty-seven are host
tooling: `serde`, `serde_derive`, `serde_json`, `serde_yaml_ng`,
`unsafe-libyaml`, `quick-xml`, `toml` + `toml_edit` + `toml_datetime` +
`toml_write` + `serde_spanned`, `winnow`, `walkdir`, `same-file`, `indexmap`,
`hashbrown`, `equivalent`, `thiserror` **1.0 and 2.0** side by side, `eyre`,
`indenter`, `once_cell`, `memchr`, `syn`, `quote`, `proc-macro2`, `itoa`,
`ryu`, `zmij`.

Timed, fresh target dir, 48-core host:

```
cargo build -p nros --target thumbv7em-none-eabihf --no-default-features --timings
units = 96   cpu = 23.5 s   wall = 7.8 s

  1.45 s  syn
  1.24 s  toml_edit
  1.23 s  ros-launch-manifest-model
  1.21 s  serde_derive
  1.15 s  nros-macros
  1.05 s  serde_core
  0.91 s  winnow
  0.76 s  ros-launch-manifest-sched
  0.65 s  serde_yaml_ng
  0.56 s  thiserror-impl
  0.56 s  nros-orchestration-ir
  0.49 s  serde_json
```

Every one of the twelve most expensive compilations in a firmware build is host
tooling for the proc-macro. Attributing each unit to the `nros-macros` subtree
by name:

```
total 23.5 s   macro subtree 20.9 s (89 %)   everything else 2.6 s
```

## Why it is not already fixed

It partly was. Phase 262 (issue 0083) cut `nros-build` — and with it the whole
of `nros-cli-core` — out of `nros-macros`, replacing it with direct deps on the
two leaf crates the macro actually needs (`nros-pkg-index`,
`nros-launch-parser`). The manifest comment records that. What remains was added
after, one dep at a time, each individually justified:

- `toml` — parse the entry package's `Cargo.toml` and `system.toml`;
- `serde_json` — read `nros sync` source-metadata sidecars (phase-307 W4);
- `nros-orchestration-ir` — shared tier schema, so macro and CLI cannot drift
  (phase-228.G);
- `ros-launch-manifest-model` — the `model = "…"` arm of `nros::main!`
  (RFC-0052 / phase-296 R2).

`nros-macros` itself is a **non-optional** dependency of `nros`
(`packages/api/nros/Cargo.toml:146`), so every consumer pays for all of it —
including a pure-library user who never writes `nros::main!`.

## Where the removable weight is

**1. The `model = "…"` arm.** `ros-launch-manifest-model` is used at six sites in
`nros-macros/src/main_macro.rs`, all inside the `model` arm, plus 28 sites in
`nros-orchestration-ir`. It is also a **git dependency** — a network fetch and
git checkout on every cold build or CI cache miss, which no crates.io mirror
helps with.

Crates reachable *only* through `ros-launch-manifest-{model,sched}`:

```
ros-launch-manifest-model, ros-launch-manifest-sched,
serde_yaml_ng, unsafe-libyaml, ryu, thiserror 2.0.18, thiserror-impl 2.0.18
```

Seven crates — 2.6 s of the 23.5 s, the same as everything that is not the
proc-macro subtree put together — and the entire reason
`thiserror` appears twice at two majors. Gating them behind a
`nros-macros` feature (default off, or default on with an opt-out) requires the
same gate in `nros-orchestration-ir`, which today has no `[features]` block at
all and deps both crates unconditionally.

**2. `toml` 0.8 → 0.9.** The workspace already resolves both:

```
toml 0.8.23  <- nros-macros, nros-orchestration-ir, nros-board-common,
                ros-launch-manifest-model, ros-launch-manifest-sched
toml 0.9.12  <- nros-tests, cbindgen (build-dep)
```

`toml` 0.8 pulls `toml_edit 0.22` → `winnow 0.7.15`, **the single most expensive
unit in the firmware build**, plus `toml_write` and `serde_spanned 0.6`. `toml`
0.9 uses `toml_parser` and does not need `toml_edit` — none of these consumers
edits TOML, they only parse it. Moving our own crates to 0.9 collapses the split
and drops five crates — but only after (1), because
`ros-launch-manifest-{model,sched}` pin 0.8 and would otherwise hold the old
tree alive.

**3. `nros-macros` optional in `nros`.** A `macros` feature (default on) lets a
consumer that hand-writes its entry point — an existing supported path — drop
all 47.

## Direction

Ordered, because each step unblocks the next:

1. Feature-gate `ros-launch-manifest-{model,sched}` in `nros-orchestration-ir`
   and `nros-macros`; the `model = "…"` arm emits a `compile_error!` naming the
   feature when off. Drops 7 crates and the git fetch.
2. Move `nros-macros`, `nros-orchestration-ir`, `nros-board-common` to
   `toml` 0.9. Drops 5 more and un-splits the resolver.
3. Make `nros-macros` optional behind a default-on `macros` feature on `nros`.

Do not attempt (2) before (1) — the git-dep pin makes it a no-op.

Measure the same two commands after each step; the acceptance number is the
57-crate count.

## Re-measured 2026-08-15 (phase-360 W5–W7)

Baseline confirmed and slightly larger: `cargo tree -e normal -p nros --target
thumbv7em-none-eabihf --no-default-features --features alloc,rmw-cffi` is **58
crates**, **47** reachable through `nros-macros`.

**The removal order this issue proposed does not survive re-derivation.**

1. **"the `model = \"…\"` arm's `ros-launch-manifest-{model,sched}` (7 crates)"
   — NOT removable that way.** Those crates are on the mainline `launch = "…"`
   path, not the deprecated override: `nros-macros/src/main_macro.rs:595` parses
   a `SystemModel` after `model_location::ensure_model(...)` (issue 0414), and
   `nros-orchestration-ir` uses them across `derive.rs`, `mapper_input.rs`,
   `rtos_realizer.rs` and `lib.rs` for the RFC-0052 tier schema. Gating one arm
   drops nothing.
2. **"then `toml` 0.8 → 0.9 (5 more)" — blocked upstream, confirmed.**
   `cargo tree -i toml` names four consumers; two are ours and two are the
   `ros-launch-manifest` git deps pinned at tag `v0.1.6`. Bumping ours leaves
   0.8 — and therefore `toml_edit` and `winnow 0.7` — alive. Needs an upstream
   bump in a fork remote.
3. **"then `nros-macros` optional" — the only viable step, and bigger than
   expected.** Measured by making the dep optional: **58 → 19 crates, 39
   dropped.** The floor is 19 rather than the "11 plus paste" this issue
   predicted, because that figure came from a narrower feature set.

**Its stated shape is illegal under the contract, though.** A default-on
`macros` feature on `nros` is unreachable: all 62 in-workspace dep-sites pass
`default-features = false`, so `check-feature-contract` clause (d) rejects it as
issue 0584's shape — a feature reachable only by whole-workspace unification,
absent from the per-package builds cmake runs. W3 (`default = []` everywhere,
dep-sites explicit) is what made that true.

The viable form is `macros` opt-in, requested per dep-site. Cost: **135 in-tree
crates** use `nros::main!` / `nros::node!` / `nros::derive::`, across 64
`packages/` and 188 `examples/` dep-sites; the examples are user-facing copy-out
projects (RFC-0026); every workspace fixture rebuilds; validation needs tier 2.

Open, with the prize (39 crates, and per the original timing the bulk of the
20.9 s macro subtree) and the price (a 135-manifest breaking change) both now
measured rather than estimated.

