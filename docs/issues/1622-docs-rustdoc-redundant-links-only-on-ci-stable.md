---
id: 1622
title: "The `docs` deploy lane fails on `rustdoc::redundant-explicit-links` in
  `nros`, the same command passes on local stable, and no gate builds the path
  that fails"
status: open
type: bug
area: ci, docs, api
severity: medium
related: [1116, 1588]
---

## What happens

`docs` run **36867095713** (push, 2026-10-01T13:13, head `85ae1d156`), job
**110385189899** (`deploy`), step 9 `Build rustdoc`:

```
error: redundant explicit link target
  = note: when a link's destination is not specified,
          the label is used to resolve intra-doc links
  = note: `-D rustdoc::redundant-explicit-links` implied by `-D warnings`
error: could not document `nros`
##[error]Process completed with exit code 101.
```

Two occurrences. The deploy publishes nothing when this fails.

## It does NOT reproduce locally, and that is the finding

The lane's command is not a workspace `cargo doc`; it is

```
RUSTDOCFLAGS="-A rustdoc::broken-intra-doc-links -A rustdoc::private-intra-doc-links" \
CARGO_TARGET_DIR=target cargo doc --manifest-path book/rustdoc-driver/Cargo.toml
```

Run verbatim on head `85ae1d156` with the repo's own stable toolchain
(**cargo 1.98.1, rustdoc 1.98.1**), it **succeeds** — `Documenting nros`,
`Finished`, `Generated …/target/doc/…`, zero occurrences of `redundant`.

The root `rust-toolchain.toml` says `channel = "stable"` with **no version**, so
CI resolves whatever stable the `ubuntu-latest` image currently carries. The lint
therefore fires on CI's rustdoc and not on 1.98.1. That is the hazard CLAUDE.md
records for clippy — *"a toolchain bump can surface NEW pre-existing lints"* —
reaching rustdoc through an unpinned channel.

## What is NOT established

**Which commit introduced it.** The lane was green on **11 consecutive runs**
ending 2026-09-29T22:19 (`efceb9c3f`) and red on its very next run, today. The
lane is push-triggered and path-filtered, so that is a **39-hour gap** containing
many commits. Attributing it to `85ae1d156` (the run's head) is not supported,
and an earlier draft of this triage did so twice before the reproduction
contradicted it. The links may equally be older prose that a newer rustdoc only
now rejects.

## The coverage gap, measured — RETRACTED the same day, see below

> **This section is wrong and is kept as a record of the reasoning.** The
> gate is green only on the LOCAL toolchain. On CI `just check rustdoc-links`
> and `just check rustdoc-workspace` both FAIL with these exact two errors —
> measured on the gates of PRs #1510, #1518, #1521, #1528 and #1529. There is
> no coverage gap; there is only the toolchain split this issue already named,
> and the conclusion drawn below from one local green does not follow.

`just check rustdoc-links` is **green on the same head** — and genuinely so, not
skipped (the first attempt skipped for want of `zenoh-pico`; provisioned, it runs
and reports `rustdoc-links OK — the 6 published crates document cleanly`). The
two invocations differ in three ways at once:

| | `just check rustdoc-links` | `docs` lane |
| --- | --- | --- |
| workspace | root | **`book/rustdoc-driver`**, its own tiny workspace |
| scope | `--no-deps`, 6 selected packages | deps documented too |
| `nros` features | `rmw-cffi,platform-posix,ros-humble,safety-e2e,std,env,macros` | `std,env,rmw-cffi,ros-humble` |

So **the artifact that gets deployed is built by a path no gate exercises.** The
`-D warnings` in the failure is not from `docs.yml` — that env only *allows* two
lints — nor from the driver manifest, nor from a `rustdocflags` in
`.cargo/config.toml`; it comes from a `[lints]` table on the documented crate,
which `RUSTDOCFLAGS` cannot override.

## Candidate sites, unverified

By the lint's definition (the explicit target resolves to what the label alone
would), in `packages/api/nros`:

```text
node.rs:979    [`CallbackCtx::integrity`](CallbackCtx::integrity)        label == target
node.rs:728    [`Executor`](crate::Executor)                            same shape, one step removed
node.rs:1187   [`Executor::register_timer_on_clock`](crate::Executor::register_timer_on_clock)
```

(A fenced block on purpose: written as prose these are live markdown links, and
`check-markdown-links` resolves them against `docs/issues/` and fails — which it
did, correctly, on the first draft of this section.)

**Measured afterwards, in PR #1529:** the first and third ARE redundant and were
fixed; `node.rs:728` is **NOT** — the bare label gives `unresolved link to
`Executor``, so its explicit target is load-bearing. Three candidates, two
errors, and the check that distinguished them was `broken-intra-doc-links` on the
same gate.

Three candidates for two errors, so this list is a starting point and not the
answer. Do not "fix" it blind: on a toolchain that does not fire the lint, a
wrong edit is indistinguishable from a right one.

## What would close it

1. Reproduce on the toolchain CI actually uses — print `rustc --version` in the
   lane (it does not today), then match it locally — and read the `-->` lines,
   which the uploaded job log omits.
2. Fix the links the compiler names.
3. Make a gate build **the driver workspace**, since that is what deploys. The
   existing `rustdoc-links` gate cannot catch this class while it documents a
   different workspace with a different feature set.
4. Consider whether `channel = "stable"` unpinned is wanted for a lane that
   denies warnings; a floating channel makes every image bump a potential red
   with no commit to attribute it to.

## It is not one lint, it is THREE, and they share this issue's cause (2026-10-01)

Measured on the gates of **#1528** (36899123494) and **#1529** (36900070917),
job `check`, which fail the SAME three steps — 17 `rustdoc-links`,
18 `rustdoc-workspace`, 33 `workspace-all`:

| lint | crate(s) | recipes it takes down |
| --- | --- | --- |
| clippy `needless_borrows_for_generic_args` | `nros-node` (`executor/spin.rs:9112`, `:9153`) | `workspace-embedded`, `workspace-all`, `test-targets`, `tier1` |
| rustdoc `redundant_explicit_links` | `nros` (`node.rs:979`, `:1187`) | `rustdoc-links`, `rustdoc-workspace`, `docs` deploy |
| rustc `semicolon_in_expressions_from_macros` | `packages/cli`: `cargo-nano-ros`, `nros-pkg-index`, `nros-launch-parser` | `cli-clippy` → `test-targets` → `workspace-all` |

Step 33 is worth naming carefully: it is `workspace-all`, and on #1528 — where
the clippy fix IS present, `needless_borrows` and `spin.rs` occurring **zero**
times in the log — it fails on `cli-clippy` / `could not compile
cargo-nano-ros`. Reading the step name alone would have misattributed it.

**All three are deny-level promotions of lints the repo's own stable does not
fire.** Local cargo/rustdoc **1.98.1** compiles and documents this tree clean;
CI resolves whatever stable `ubuntu-latest` carries, because the root
`rust-toolchain.toml` says `channel = "stable"` with no version. That is this
issue's mechanism, not confined to rustdoc.

### What it cost

Every open PR fails all three — measured on #1510, #1518, #1521, #1528, #1529 —
so **nothing can merge**. Main has been unmoved since 13:13 with an empty merge
queue and ~20 PRs blocked. One image bump stopped the repo, and no commit
introduced it: the `docs` lane was green on 11 consecutive runs to
2026-09-29T22:19 and red on its next.

### The third lint is a DECISION, not an edit sweep

`semicolon_in_expressions_from_macros` fires inside `anyhow::bail!` used in
**expression** position (`Language::C => bail!(…)`, `other => bail!(…)`). The
gate log carries **184** occurrences, at least at:

```
cargo-nano-ros/src/scaffold.rs:157              nros-launch-parser/src/lib.rs:376,394,410,453,493,680
cargo-nano-ros/src/workspace_scaffold.rs:237,268,276     nros-pkg-index/src/lib.rs:98,123,348
src/main.rs:129    packages/cli/third-party/play_launch/.../launch_dump.rs:334
```

One is **vendored** (`third-party/play_launch`), which this repo does not patch
in place. So the remedy is a choice — allow the lint in `packages/cli`, bump
`anyhow` to a release whose `bail!` has no trailing semicolon, or restructure
every expression-position `bail!` — and it belongs with whoever owns
`packages/cli`.

### Added to what would close it

**Pin the toolchain, or stop denying warnings.** Unpinned `channel = "stable"`
plus `-D warnings` means any runner-image rustc bump can red every lane at once,
with no commit to bisect and no local reproduction. Items 1–3 above still stand
for the rustdoc lint specifically; this is the class.

PRs **#1528** (clippy) and **#1529** (rustdoc) are correct and verified but
**cannot merge alone**, because the gate runs all three steps.
