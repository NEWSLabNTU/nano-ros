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

## The coverage gap, measured

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

- `src/node.rs:979` — ``[`CallbackCtx::integrity`](CallbackCtx::integrity)``,
  label identical to target.
- `src/node.rs:728` — ``[`Executor`](crate::Executor)``, and `src/node.rs:1187`
  ``[`Executor::register_timer_on_clock`](crate::Executor::register_timer_on_clock)``
  are the same shape one step removed.

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
