---
id: 1921
title: "An `NROS_BUILD_ROOT` outside the checkout is shared by every checkout that sets it, for every build-cache kind except the metadata probe: fixture trees, the sizes probe, Corrosion cargo dirs and the lane skip ledgers all span checkouts"
status: resolved
type: bug
area: [build, testing]
severity: low
found: 2026-10-15
resolved: 2026-10-15
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

## Resolution

Fixed 2026-10-15, in the direction filed, with one addition: the rule is
idempotent.

**One rule, one spelling per build system.** A build root the checkout does
not own — outside every checkout, or inside ANOTHER checkout — becomes
`<root>/checkouts/<basename>-<sha256(realpath)[:12]>` (issue 1904's spelling,
so 1904's probe dirs under a foreign root keep their path). "Owned" is the
deepest marker-walk owner after resolving symlinks (the 1280/1391 rule) **or**
a root that already lies inside this checkout's own `checkouts/<name>` scope.
The second arm makes the rule idempotent, which is what lets every consumer ask
it without double-scoping: `nros_build_dir corrosion-cargo …` hands cmake an
already-scoped `NROS_SHARED_CARGO_ROOT`, and cmake asks the same question and
gets the same path back. An owned root, including the default
`<checkout>/build`, is unchanged, so no warm cache is orphaned.

- shell: `nros_checkout_scoped_root` in `scripts/lib/checkout-paths.sh`,
  called by `nros_build_root` (and `export -f`'d with it to make leaves);
- Rust: `nros_build_paths::checkout_scoped_root` / `build_root{,_in}`, which
  `nros_tests::build_root` and the metadata probe call;
- cmake: `nros_checkout_scoped_root()` in `cmake/NanoRosCheckoutRoot.cmake`,
  called by `nros_shared_cargo_dir`;
- python: no fourth spelling — `scripts/lib/build_root.py` runs the shell.

**1904's per-site scoping is retired**, not kept beside the rule. The probe's
private `checkout_scoped_root` is gone (it asks `nros_build_paths::build_root`),
and the cmake key no longer carries `checkout=<path>`: the ROOT is scoped, so
the key is configuration-only again. The one cache that moves is a Corrosion
dir under a raw foreign `-DNROS_SHARED_CARGO_ROOT` passed by hand (from
`<root>/<hash-with-checkout>` to `<root>/checkouts/<name>/<hash>`), a layout
that existed for one day.

**Second derivations found by the sweep** (`git grep` for a raw read of the
variable in `*.sh *.py *.rs *.just *.cmake`): `check-action-client-arena-budget.py`
and `check-size-markers-unlinked.py` read `os.environ["NROS_BUILD_ROOT"]`,
`gc-shared-cargo.py` used a `<repo>/build/corrosion-cargo` literal that never
followed the variable at all, `tests/fixture-staleness-probe-tests.sh` joined
`$NROS_BUILD_ROOT/cargo-fixtures/linux` by hand, the metadata probe read it
itself, and a `nros_tests` unit test read it to decide whether to skip. All now
go through the one derivation.

**Gates, each mutation-proven** (shell half dropped → `build_root_derivation.sh`
rc=1; Rust half dropped → `nros_build_paths` tests rc=101 and the derivation
gate rc=1; cmake half dropped → `check-cargo-dir-knob-key` rc=1; a raw read
reintroduced → derivation gate rc=1):
- `build_root_derivation.sh` — four real-directory cases (outside, another
  checkout, this checkout, already scoped) plus the make-leaf `export -f` case;
  and a sweep refusing a direct read of the variable outside the two
  definitions;
- `nros_build_paths` `checkout_scope_tests` — RUN the shell function against
  the Rust one on the same four cases (the derivation gate is buildless and
  cannot run Rust);
- `check-cargo-dir-knob-key` — the cmake answer equals the shell answer on four
  roots;
- `check-inherited-checkout-paths` — its assertion that the variable passes
  through verbatim is flipped.

**Measured.** From this worktree and a synthetic second checkout (a directory
carrying the marker and a copy of `scripts/`), one shared `NROS_BUILD_ROOT` in a
scratch dir: a `check-skips` ledger write landed in
`<root>/checkouts/p1921-294755001d3f/check-skips/` and
`<root>/checkouts/ck2-96fdc6eb5083/check-skips/` respectively, and the sizes
probe dir resolved under each; a third synthetic checkout carrying origin/main's
`build-root.sh` wrote `<root>/check-skips/` — the shared ledger this fixes.

**Not measured.** Two real checkouts building fixtures into one root; whether a
fixture `.inputsig` would have been fooled by a sibling's stamp (moot once the
trees are separate). `scripts/ci/runner-sweep.sh` now measures this checkout's
subtree of a relocated root rather than the whole root; no runner sets the
variable today.
