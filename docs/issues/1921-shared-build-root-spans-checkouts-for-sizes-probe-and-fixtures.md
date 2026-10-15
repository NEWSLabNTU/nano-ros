---
id: 1921
title: "An `NROS_BUILD_ROOT` outside the checkout is shared by every checkout that sets it, for every build-cache kind except the metadata probe: fixture trees, the sizes probe, Corrosion cargo dirs and the lane skip ledgers all span checkouts"
status: open
type: bug
area: [build, testing]
severity: low
found: 2026-10-15
related: [1904, 0616, 0805, 0464, 0650, 1280]
---

## What was seen

Issue 1904 made the metadata probe's shared dirs carry their checkout: a build
root the checkout does not own is scoped to `<root>/checkouts/<name>-<hash>/`
(Rust) or keyed with `checkout=<realpath>` (CMake, every Corrosion lane). While
sweeping for siblings it found that the rest of the build cache has no such
rule.

`nros_build_root` (`scripts/build/build-root.sh`) returns `$NROS_BUILD_ROOT`
verbatim when it is set, and its Rust twin `nros_tests::build_root` does the
same. Every RFC-0070 kind lives under that root, so two checkouts that export
the same `NROS_BUILD_ROOT` share all of them:

- `cargo-fixtures`, `cmake-fixtures`, `west-fixtures`, `compile-check-fixtures`
  — the fixture trees and their stamps.
- `sizes-probe` — `scripts/build/cargo.sh` exports
  `NROS_SIZES_PROBE_TARGET_DIR=$(nros_sizes_probe_dir)`, keyed by
  (rustc slug, target, features) and not by checkout.
- `lane-skips` / `check-skips` (issue 0650) — one ledger file per lane, which a
  lane's terminal recipe reads to refuse claiming work it skipped.
- the rest (`example-lint`, `example-build`, `stack-analysis`, …).

## Why it matters

- **Duplicate units, shared nothing (issue 0616's shape).** Each checkout
  reaches the same crates by its own absolute path, and `-C metadata` includes
  that path, so a shared cargo target dir holds two units of every crate and
  reuses none of them, while the two checkouts contend for one cargo lock.
- **Ledgers and stamps answer for the wrong tree.** A lane in checkout A can
  read a skip ledger or fixture stamp that checkout B wrote. Whether any
  freshness check is fooled by this was NOT measured; the fixture
  `.inputsig`s hash their own inputs, which may contain it.

What it is NOT: the sizes probe no longer picks an rlib by mtime. Issue 0464
removed that fallback; the probe reads the rlib path from cargo's
`compiler-artifact` JSON, so a shared dir cannot make it return another
checkout's build.

## Measured / not measured

- Read from the code at the time of filing: `nros_build_root`, the kind list in
  `build-root.sh`, `cargo.sh`'s export, `nros-sizes-build`'s dir choice.
- **Not run:** two checkouts with one shared `NROS_BUILD_ROOT`. Nothing in the
  repository exports it; it is an operator choice ("move the whole cache to a
  faster volume"), so this bites only someone who sets it and works in more
  than one checkout.

## Fix direction

Apply issue 1904's ownership rule ONCE, in `nros_build_root` and
`nros_tests::build_root` together: a root the current checkout does not own
(deepest checkout by the marker walk, after resolving symlinks — the
1280/1391 rule) becomes `<root>/checkouts/<name>-<hash>`. That scopes every
kind at once and retires 1904's per-site scoping in favour of one rule. A root
the checkout owns (the default `<checkout>/build`) is unchanged, so no warm
cache is orphaned.

Gate: `build_root_derivation.sh` already pins the shell and Rust derivations to
each other; add a case where the root is outside the checkout, asserting both
halves scope it identically.
