---
id: 379
title: "No lane runs clippy on the `packages/cli` sub-workspace, so ~30 lints have accumulated there under rust 1.97"
status: open
type: tech-debt
area: build
related: [issue-0319, issue-0202]
---

# The `packages/cli` sub-workspace is never clippy'd

## Summary

`packages/cli/` is its own Cargo workspace with its own `Cargo.toml` /
`Cargo.lock`. Every clippy invocation in the repo targets the ROOT workspace:

```
$ grep -rn "packages/cli/Cargo.toml" justfile just/*.just .github/workflows/*.yml | grep -i clippy
(no output)
```

`check-cli-fmt` formats it and `check-cli-tests` runs its tests (added by issue
0202, after that suite sat red for months unnoticed), but nothing lints it. So
the lints simply accumulate. Running it by hand today, on rust 1.97:

```
$ cargo clippy --manifest-path packages/cli/Cargo.toml --workspace --release -- -D warnings
error: usage of `contains_key` followed by `insert` on a `BTreeMap`   nros-msg-to-idl/src/emitter.rs:74
error: this `if` statement can be collapsed                            nros-msg-to-idl/src/mangle.rs:74
error: this manual char comparison can be written more succinctly      nros-msg-to-idl/src/mangle.rs:226
error: match expression looks like `matches!` macro                    nros-msg-to-idl/src/mangle.rs:332
error: this `if` statement can be collapsed                            nros-msg-to-idl/src/parser.rs:74,155
error: use of `extend` instead of `append`                             nros-msg-to-idl/src/parser.rs:170
error: stripping a prefix manually                                     nros-msg-to-idl/src/parser.rs:246
error: called `unwrap` after checking `is_some`                        nros-msg-to-idl/src/types.rs:77
error: needless borrow (×3)                                            rosidl-codegen/src/generator/srv.rs:580, …
… ~30 total across nros-msg-to-idl, rosidl-codegen, nros-cli-core
```

None are correctness bugs at a glance — mostly style plus a few
`unwrap`-after-`is_some` patterns worth a look. `nros-cli-core` itself is
comparatively clean.

## Why it matters

This is the **silent-lane class** the repo has been bitten by twice: issue 0202
(nothing ran `cargo test` on `packages/cli`) and issue 0319 (the Cyclone suite
was not on any `check-*` lane, so a red sat on main for two days). A gate that
covers "the workspace" but silently means "the root workspace" is the same
shape. It also means a toolchain bump — the exact scenario CLAUDE.md warns
about, where a new rustc surfaces pre-existing lints — lands here invisibly and
the debt only shows up when somebody adds the lane.

Latent, not red: because no lane runs it, main is not failing today.

## Direction

1. Add `check-cli-clippy` to `check-fast` (or `check-build`, it costs a
   compile), running `cargo clippy --manifest-path packages/cli/Cargo.toml
   --workspace -- -D warnings`.
2. That lane cannot go green until the existing ~30 lints are fixed, so land
   the cleanup first, in one mechanical commit per crate.
3. While doing it, check the `unwrap`-after-`is_some` sites for real
   nullability bugs rather than rewriting them mechanically.

Found while running clippy over a `cmd/setup.rs` change for issue 0374; the
change itself is lint-clean, which is how the surrounding noise became visible.
