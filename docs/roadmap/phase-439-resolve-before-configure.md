# Phase 439 — Resolve before configure

**Status (2026-09-08). Opened from RFC-0094. W0 and W1 are the routing diff and
the digest key — both are PRECONDITIONS and neither changes behaviour. W2–W4 are
the three landings. W0 has LANDED and corrected three of the RFC's numbers; W1
is not started.**

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

## W0 — The routing diff (acceptance A1) — **LANDED 2026-09-08**

`scripts/check/check-package-routing.py`, wired as the fast gate
`just check package-routing`. It pushes every tracked `package.xml` through
RFC-0094 D3's rule and diffs the resulting member / subdirectory lists against
what the three file-presence sites produce today.

* `builder/cargo_root.rs` — `[workspace] members`
* `builder/cmake_root.rs` — `add_subdirectory()`
* `cmd/build.rs` — the cargo-vs-cmake driver choice

Buildless (`git ls-files`), 0.05 s, no baseline list. The full table is
`python3 scripts/check/check-package-routing.py --report`.

### What it measured

The four bucket counts held exactly, on 416 tracked `package.xml`:

    both CMakeLists.txt + Cargo.toml :  21
    CMakeLists.txt only              : 173
    Cargo.toml only                  : 158
    neither                          :  64

**21 packages change side, all of them the class D3 named** — a dual-file
package declaring `nros_cmake`, leaving the cargo members list. Nothing else
moves, and no package's declared type contradicts the single build file it
carries: every `cmake-only` package declares a cmake type and every
`cargo-only` package a cargo type. That last is the finding that makes D3 cheap
— the two questions have not actually diverged anywhere yet.

### Three corrections to the RFC's numbers

* **416 tracked `package.xml`, not 411.** The RFC's four bucket counts sum to
  416 and its prose says 411; the buckets are right.
* **The declare-but-no-file class is 64, not 20.** Composition: 23 `nros_cargo`,
  22 `nros_cmake`, 12 `ament_cmake`, 2 `ament_cargo`, 5 declaring nothing. It is
  not an interface-package accident — it is three systematic families: **34
  bringup packages** (launch + `system.toml`, no build file of their own; 32
  carry a `system.toml`), **13 descriptor packages** (`config/*`,
  `packages/boards/*` carrying `nros-platform.toml` / `nros-board.toml`), and
  **17 interface / message packages**. So D3's
  reason for keeping participation on file presence is three times stronger than
  the RFC states, and the `<build_type>` on all 59 of these is decorative today
  — a bringup declares whatever its workspace's language is, by copy-paste.
* **"Mostly Zephyr Rust leaves" understates how already-fixed the 21 are.** 20 of
  the 21 carry an independent reason cargo membership is impossible or already
  waived, and the gate prints which:
  * **13 declare their own `[workspace]`** — listing one as a member of another
    root is a hard cargo error, measured against a synthetic two-manifest tree:
    `error: multiple workspace roots found in the same workspace`.
  * **13 declare `[package.metadata.nros.entry] deploy`**, which
    `cargo_excluded_entry_dirs` already resolves through the board catalog to
    `Driver::West` and excludes. (Six carry both.)

  So for the 7 workspace Zephyr entries, D3 changes nothing observable: it
  reaches today's answer from the declaration instead of a `Cargo.toml`
  metadata round-trip through the board catalog. The genuine repair is
  `examples/workspaces/mixed/src/rust_heartbeat_pkg` — a cmake-driven Rust node
  (`nano_ros_node_register … LANGUAGE RUST SOURCES Cargo.toml`) that declares
  its own `[workspace]`, so the moment the `mixed` workspace routes to the cargo
  driver the generated root is unusable. That one is a **latent fix the RFC did
  not name**.

### The one row that is neither, and blocks W3

`packages/rmw/cyclonedds/nros-rmw-cyclonedds` — issue 1224. It is the only
dual-file package with no own `[workspace]` and no entry `deploy`, and it is
member 87 of the repo-root `[workspace]`. Its `package.xml` declares
`nros_cmake` while cargo genuinely builds the crate; the `CMakeLists.txt` is a
separate C/C++ test wrapper. Nothing breaks today only because no workspace walk
reaches `packages/rmw/`. W3 makes that declaration load-bearing, so the
declaration is what must change.

**Acceptance: met.** No build, 21 named-and-justified rows, wired as a gate.
Mutation-tested on the real tree — flipping
`examples/workspaces/rust/src/zephyr_entry` to `nros_cargo` reds the gate naming
it (`{cargo-member, cmake-subdir} -> {cargo-member}`), and flipping
`examples/workspaces/c/src/talker_pkg` to `nros_cargo` reds it on the
intersection rule (`declares a cargo build type and carries no Cargo.toml`);
both restore green.

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
