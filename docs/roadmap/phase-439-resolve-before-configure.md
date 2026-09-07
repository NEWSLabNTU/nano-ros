# Phase 439 — Resolve before configure

**Status (2026-09-08). Opened from RFC-0094. W0 and W1 are the routing diff and
the digest key — both are PRECONDITIONS and neither changes behaviour. W2–W4 are
the three landings. Nothing here is started.**

Implements [RFC-0094](../design/0094-resolve-before-configure.md). Read its
"Design" section first; this doc carries work items and acceptance only.

## Why the order is what it is

Two of the five items exist because a simulation of the proposed workflow found
flaws in it before any code was written. They are listed first for that reason,
not by size.

* **W0 before W3** — routing on `<build_type>` alone hard-fails 20 packages that
  declare a type and legitimately have no build file. The routing diff is the
  test that finds them, and it needs no build.
* **W1 before W2** — the resolve phase makes per-image knob divergence normal;
  without the digest in every cargo-dir key, two images differing only in a
  knob share one directory and the second silently gets the first's numbers.
  RFC-0094 D4. **The phase is worse than the status quo without it.**

## W0 — The routing diff (acceptance A1)

A script that pushes every tracked `package.xml` through RFC-0094 D3's rule and
diffs the resulting member / subdirectory lists against what the three
file-presence sites produce today.

* `builder/cargo_root.rs` — `[workspace] members`
* `builder/cmake_root.rs` — `add_subdirectory()`
* `cmd/build.rs` — the cargo-vs-cmake driver choice

Every package that changes side is NAMED and classified as a fix or a
regression. Known expectations, to be confirmed rather than assumed:

* the 21 dual-file packages (mostly Zephyr Rust leaves declaring `nros_cmake`)
  should LEAVE the cargo members list — a fix;
* the 20 declare-but-no-file packages should stay out of both — no change;
* everything else should be identical.

**Acceptance:** the script runs with no build, the diff is empty except for
named-and-justified rows, and it is wired as a gate so the rule cannot drift
back.

## W1 — The resolve digest in every cargo-directory key

`nros_share_corrosion_cargo_dir(KEY …)` currently keys on
`features, rmw, board, caps, profile, target`. The Zephyr lane already adds
every `NROS_RESOLVED_*`; the `nros-c` and NuttX lanes do not.

**Acceptance:** two images differing ONLY in a derived knob resolve to different
cargo directories. Mutation-tested — revert the key change and the collision
reappears, demonstrated rather than argued.

## W2 — Stage 3.5, the resolve phase

The phase itself: read descriptors and declared entities, run
`EntityInventory::derive` once, write `build/<image>/resolved.toml` with
`[provenance]`. Stages 4 and 5 read it and never re-derive.

Then delete what it replaces: the three-pass convergence in
`zephyr/cmake/nros_cargo_build.cmake`, `nros_reconfigure_settle`, and the
future-mtime arm in `cmake/NanoRosReconfigure.cmake`.

**Acceptance (A3):** Zephyr converges in ONE pass; `check-knob-delivery` still
answers for a built dir; a named image's knobs are byte-identical to today's
before the deletion and after it.

**Known gap, not a blocker:** `ZPICO_MAX_QUERYABLES` and
`ZPICO_MAX_LARGE_SUBSCRIBERS` have no declarative derivation (issues 0827, 1061,
1125). They stay hand-set and `[provenance]` says so, which is strictly better
than being hand-set and silent.

## W3 — `build_type` selects the driver

Add `build_type` to `PackageXml` and have the three sites read it for the
DRIVER, keeping file presence for PARTICIPATION (RFC-0094 D3). Gate the
intersection: a participating package must have the files its declared type
needs.

**Acceptance:** W0's diff is empty; a package declaring `nros_cargo` with no
`Cargo.toml` that IS routed produces a loud error naming the package, where
today it is silently skipped.

## W4 — Descriptors become load-bearing

`nros_rmw_dispatch` becomes a query against the provider index rather than a
generated `if/elseif` chain, following `NanoRosProviders.cmake`'s existing
"cmake asks the CLI" pattern. Both closed lists go — the generated one at
`cmake/NanoRosRmwDispatch.cmake:52` and the hand-written one at the root
`CMakeLists.txt:400`.

Every new configure-time query goes through `nros_codegen_tool_reconfigure()`
(RFC-0094 D6, issue 1018).

**Acceptance (A2):** a fifth backend — a `package.xml` provision, an
`nros-rmw.toml`, a `CMakeLists.txt`, and a C `nros_rmw_acme_register` —
configures and links with ZERO edits to `NanoRosRmwDispatch.cmake`, the root
`CMakeLists.txt`, or the `nros` binary. `uorb` becoming selectable is the
in-tree proof.

Closes issues 1214, 1215, 1216, 1219.

## Out of scope

Per-package independent builds (RFC-0094 "Out of scope"), the `std` deletion,
the unsafe census, and workspace membership. Filed as 1208–1221; none blocks
this phase.
