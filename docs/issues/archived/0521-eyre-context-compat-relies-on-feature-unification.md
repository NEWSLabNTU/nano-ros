---
id: 521
title: "`eyre::Context::with_context` on a Result is a feature-gated compat surface: two CLI crates compiled only because a workspace sibling enabled it"
status: resolved
type: bug
severity: high
area: cli, build
related: [issue-0505, issue-0499, phase-172]
resolved_in: "issue-0521 (eyre-native wrap_err/wrap_err_with in the crates consumed outside the CLI workspace)"
---

## Symptom

`check-cli-tests` fails, so `ci-matrix` (tier 2) stops before `test-all`:

```
metadata-mode harness failed (exit 101) for component 'demo_pkg::talker':
error[E0599]: no method named `with_context` found for enum `Result<T, E>` in the current scope
```

The excerpt the CLI prints names only the first diagnostic and no file, which
reads as a broken harness. Building the harness crate the test leaves behind
(`tmp/metadata_build-*/probe`) gives the real location:

```
error[E0599]: no method named `with_context` found for enum `Result<T, E>`
    --> packages/cli/nros-pkg-index/src/lib.rs:158:10
help: trait `ContextCompat` which provides `with_context` is implemented but
      not in scope; perhaps you want to import it
```

Then the same again in `nros-launch-parser`, 7 more call sites.

## Cause

eyre splits two traits:

* `WrapErr` — `wrap_err` / `wrap_err_with` on `Result`. Native, always available.
* `ContextCompat` (re-exported as `Context`) — `context` / `with_context`, the
  ANYHOW-COMPATIBILITY surface, **feature-gated**.

`nros-pkg-index` and `nros-launch-parser` both write
`use eyre::{Context, …}` and call `.with_context(…)` on `Result`. Neither
declares the feature that provides it — both say plain `eyre = "0.6"`. They
compiled anyway because some OTHER member of the `packages/cli` workspace turns
it on, and cargo unifies features across the workspace.

Both crates are also consumed from OUTSIDE that workspace: the metadata harness
generates a crate depending on `packages/api/nros`, which reaches them. In that
smaller graph nobody enables the feature, so every call site fails at once.

This is a build that works only because of who else is in it — the same shape as
a leaf that resolves a dependency through a sibling's `[patch]`.

## Fix

Both crates now use eyre's native spelling — `wrap_err_with` for `Result`, and
`wrap_err` where `.context(…)` was used. No feature needed, so the result cannot
depend on graph composition. 6 sites + 2 in `nros-pkg-index`, 7 in
`nros-launch-parser`.

Verified: the harness crate builds, and `plan_pipeline_e2e` is 3 passed / 0
failed (it was 2 passed / 1 failed).

## The first cut fixed two crates and called the rest latent — wrongly

I stopped after `nros-pkg-index` and `nros-launch-parser` on the reasoning that
the remaining thirteen files live in `nros-cli-core`/`nros-build`, "only ever
built inside the CLI workspace". The very next `lane=all` disproved it: the
compile-check fixtures build `nros-cli-core` too, and it failed in `ament.rs`
and `workspace.rs` — the latter as a confusing `E0277: the size for values of
type 'str' cannot be known at compilation time`, which is what a missing
`ContextCompat` looks like once resolution falls through to another candidate.

Fixing the reported site instead of the class cost one full fixture lane.

## Swept

All 13 remaining files converted — `.with_context(` → `.wrap_err_with(`,
`.context(` → `.wrap_err(`, `use eyre::{Context, …}` → `{WrapErr, …}`. Every
`.context(` receiver was checked first and all were `Result`s (serde_json, io,
`plan_system`), so no ContextCompat use was lost; a receiver that IS an `Option`
must keep the compat trait and be imported explicitly.

Sweep + verification:

```
git grep -c "\.with_context(\|\.context(" -- packages/cli   # expect: no output
cd packages/cli && cargo check --workspace && cargo test --workspace
```

The durable alternative, if this recurs: have the crates that genuinely want the
compat surface declare the eyre feature themselves rather than inherit it.

## Reporting

`first_diagnostic` gives one line with no file path, which is what made this look
like a harness defect rather than a compile error in a named crate. The harness
does preserve its scratch crate, so `cd tmp/metadata_build-*/probe && cargo
build` is the way to see the real errors — worth knowing before the next one.
