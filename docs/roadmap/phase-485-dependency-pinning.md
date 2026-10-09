# Phase 485 — dependency pinning: one pin set, one project lock

**Status (2026-10-10). All work items open; measurements first.** Implements
[RFC-0104](../design/0104-dependency-pinning-and-project-lock.md). The RFC's
D3 and D5 rest on facts not yet measured, so M1–M3 run before any
implementation and may amend the RFC.

## Measurements (before W1)

### M1 — does cargo honour a SEEDED lock? (RFC-0104 D3)

On 3–4 real leaves — one native, one embedded (cross target), one with message
deps, one with platform-conditional deps — write a SUPERSET `Cargo.lock` from
the union of the tree's registry entries, run `cargo metadata`, and record:
are seeded versions kept where the manifest accepts them; are unused entries
(including ones for targets the leaf does not build) pruned; are generated
`0.0.0` path crates added without touching registry entries; is the result
identical across two hosts with different ament installs. Decides open
question 1 (pin-file format).

### M2 — does today's build regenerate after an ament change? (RFC-0104 D5)

Under a scratch ament prefix, bump a msg package's `package.xml` version and
add a field to a `.msg`; rebuild a native Rust leaf, a C/C++ leaf and a
workspace entry with no other change. Record whether generated code is
regenerated, by which edge, and whether the image changes. A NO is a live
defect: file it, and it becomes W5's acceptance.

### M3 — which C/C++ source reaches which image, by which road

Derive, do not author: per fixture row, which `[source.*]` / submodule trees
the build reads (cmake `CMAKE_CONFIGURE_DEPENDS` + depfiles, cargo
`rerun-if-changed`, west module lists). The `[source.*]` section of the project
lock (W3) is computed from this, never hand-listed.

## Work items

### W1 — the registry pin set (D2)

- Generate `locks/cargo-registry.lock` as the union over every root (repo
  root, `packages/cli`, every leaf with a `Cargo.toml`); the index names it
  (`[rust] registry_pins`).
- Gate: the tracked root and CLI `Cargo.lock`s are subsets of it.
- Gate: a `[source.*]` entry is submodule-mode (no `ref`) XOR clone-mode
  (`ref`), never both.

**Acceptance:** one tracked registry pin file; both subset gates fail on a
planted mismatch.

### W2 — seeded leaf locks (D3)

`nros sync` / `nros build` seed each leaf `Cargo.lock` from the pin set and
resolve through cargo. Delete the tracked leaf locks under `packages/` that the
seed replaces; retire the `check-leaf-lockfiles` tracked-lock invariant.

**Acceptance:** every example leaf builds from a seeded lock on a fresh clone;
two hosts with different ament installs produce identical registry sets.

### W3 — the project lock across kinds (D4)

Extend `SdkLock` (`nros-sdk.lock`) with `[source.*]` (commit + origin, from
M3), `[cargo]` (registry set), `[system.*]`, `[generated.*]`. Written on every
build, only when content changes (the existing write-if-changed).

### W4 — the drift check (D9)

Project lock vs pin set, kind by kind, as a set comparison after resolution;
a hard error naming `nros update`. Retire the `scripts/bin/cargo` shim; move
`--locked` into the recipes that build the repo root and the CLI. Remove the
shim's wiring from `activate.sh` and its gates (`check-*` naming the shim).

**Acceptance:** a planted out-of-pin crate, source commit and tool sha each
fail; `NROS_CARGO_FLAGS` no longer exists.

### W5 — regenerate on ament change (D5)

`[generated.<pkg>] ament` = hash of the ament inputs + codegen version; a
mismatch re-runs generation for that package before the build. Acceptance is
M2's scenario turning green.

### W6 — record and warn for system libraries (D6)

Record `[system.*]`; on change, warn once naming what moved and what
regenerated. Add compatibility ranges to the index (open question 2) for the
zenoh router (`rmw_zenoh_cpp`'s zenoh vs zenoh-pico) and host Cyclone vs the
fork's 0.10.5 line; an observation outside a range warns loudly.

### W7 — `nros update` (D7)

`--crate` (per-root `cargo update -p --precise`, merge, re-seed, move root/CLI
locks), `--source` (writes the gitlink or `ref`), `--tool` (index version +
sha256). Replaces `just lock-update`. Breaking bumps call `cargo upgrade` when
present. Duplicate majors reported via `cargo tree -d`.

**Acceptance:** the RFC's heapless walkthrough, both variants, as a test over a
scratch tree.

### W8 — no fetch at build time (D10)

Remove Corrosion's `FetchContent` fallback from `cmake/NanoRosCorrosion.cmake`;
a configure without the pinned Corrosion refuses and names `nros setup`. Gate:
no `FetchContent_Declare` / `ExternalProject_Add` outside `third-party/`.

### W9 — developer tools (D8)

`cargo-deny` (config over the pin set: duplicates, RUSTSEC, licences, sources)
and `cargo-edit` as `[tool.*]` store entries or `just dev-tools`; a `just
check deny` lane. Optionally `cargo vendor` for the offline nightly
(issues 0873/1356).

### W10 — released SDK (open question 4)

The pin set ships beside the index in the SDK root (RFC-0099 D2); a user
project's first `nros build` writes `nros-sdk.lock` (RFC-0095 D9) and never
rewrites a pinned entry without `nros update`.

## Order

M1–M3 → W1 → W2 + W3 → W4 → W5 (or earlier if M2 finds a live defect) → W6,
W7 → W8, W9, W10 (independent).

## Out of scope

- The submodule FORWARD-ONLY rule (`check-submodule-pins`) — history, not
  drift; it stays as is.
- Pinning system libraries — rejected (RFC-0104 D6, issue 1248).
