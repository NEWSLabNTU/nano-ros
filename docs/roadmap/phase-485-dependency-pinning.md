# Phase 485 — dependency pinning: one pin set, one project lock

**Status (2026-10-10). M1–M3 measured (results below); the issue-1781 fix is in review;
all work items open.** Implements
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

**Result (2026-10-10) — seeding holds.** Seed = registry-only union of the 39
tracked locks (657 entries). Per leaf, a FRESH resolve (`cargo
generate-lockfile`) vs a SEEDED one (the union copied in, then `cargo
metadata`):

| leaf | road / target | registry crates | fresh: moved / new | seeded: kept | seeded lock |
| --- | --- | --- | --- | --- | --- |
| `native/rust/logging` | cargo, host | 25 | 0 / 0 | 25 / 25 | 33 pkgs (657 pruned to the graph) |
| `mps2-an385-baremetal/c/talker` | cargo-rooted C, thumbv7m | 60–62 | 4 / 1 (`cortex-m` 0.7.9, `libc` 0.2.190, …) | 60 / 60 | 97 pkgs |
| `native/rust/talker` | cargo, msg deps (generated `std_msgs`) | 94 | 5 / 0 (`jiff`, `libc`, `zerocopy`) | 94 / 94 | 136 pkgs |

- Cargo KEEPS seeded versions the manifests accept and PRUNES the rest. With
  an oldest-version-only seed on `logging`, seeded kept 23/25 vs fresh 10/25;
  the 2 that moved (`indexmap` 1.9.3, `hashbrown` 0.12.3) were seeded at a
  major the manifest does not accept — exactly what the drift check reports.
- Where the seed holds several semver-compatible versions, cargo takes the
  HIGHEST seeded one (`libc` 0.2.189 in both leaves that use it) —
  deterministic.
- Generated `0.0.0` path crates were added without moving any registry entry.
- **Finding:** the union of today's tracked locks holds **84 crates with more
  than one semver-compatible version** — the tracked locks already disagree
  with each other. The pin file holds ONE version per semver-compatible range
  (open question 1 answered: a registry-only `Cargo.lock`, normalised to one
  per range).
- Not measured: two hosts with different ament installs (one host here). The
  generated crates being `0.0.0` path deps makes the registry set
  host-independent by construction; re-check when a second host is to hand.

### M2 — does today's build regenerate after an ament change? (RFC-0104 D5)

Under a scratch ament prefix, bump a msg package's `package.xml` version and
add a field to a `.msg`; rebuild a native Rust leaf, a C/C++ leaf and a
workspace entry with no other change. Record whether generated code is
regenerated, by which edge, and whether the image changes. A NO is a live
defect: file it, and it becomes W5's acceptance.

**Result (2026-10-10) — NO, and a second defect behind it.** On
`native/rust/talker` (cargo road): after an ament change, `nros build` alone
did not regenerate and produced a byte-identical binary; only an explicit
`nros sync` regenerated. Filed as
[issue 1780](../issues/1780-nros-build-keeps-generated-code-after-ament-change.md)
— W5's acceptance. Getting there found
[issue 1781](../issues/archived/1781-ament-index-last-prefix-wins.md): the CLI's ament
index lets the LAST `AMENT_PREFIX_PATH` entry win, so an overlay's interface
package is silently replaced by the underlay's — the overlay had to be put
LAST for the experiment to see it at all. The cmake and west roads are not
yet measured.

### M3 — which C/C++ source reaches which image, by which road

Derive, do not author: per fixture row, which `[source.*]` / submodule trees
the build reads (cmake `CMAKE_CONFIGURE_DEPENDS` + depfiles, cargo
`rerun-if-changed`, west module lists). The `[source.*]` section of the project
lock (W3) is computed from this, never hand-listed.

**Result (2026-10-10).** A script over what each BUILD recorded — `ninja -t
deps` + `ninja -t inputs` (cmake, west), build-script `rerun-if-changed` +
rustc dep-info (cargo) — attributing each path to the `[source.*]` root that
owns it:

| image | sources read | from |
| --- | --- | --- |
| native C / C++, zenoh | zenoh-pico | the CARGO half only — the cmake half reads no vendored source |
| native C / C++, Cyclone | cyclonedds-src (331 files) | ninja |
| native C / C++, XRCE | micro-xrce-dds-client, micro-cdr | the cargo half |
| native Rust talker | zenoh-pico (287 files) | cargo |
| Zephyr FVP entry (Cyclone) | cyclonedds-src (314 files) + west modules `lang-rust`, `cmsis`, `mbedtls`, `mcuboot`, `picolibc`, `tinycrypt` | ninja + `zephyr_modules.txt` |

1. A cmake image's set is ninja ∪ its CARGO half: the RMW's vendored C is
   compiled by the cargo half, so ninja alone missed zenoh-pico and XRCE.
2. West modules are a seventh kind, pinned by `west.yml`, not the index —
   RFC-0104 D4a (added from this result).
3. Declared is not used: `mbedtls` is in the index and no image measured read
   it (TLS off) — the lock section must be derived, as D4 now says.
4. Not measured: FreeRTOS, ThreadX, NuttX, lwIP, NetX Duo, PX4 — no current
   build of them on this host (the only NuttX tree was a stale untracked
   directory). W3's derivation must be run over each before it is trusted
   there.

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

W3 also records `[west.<module>]` for a Zephyr image (RFC-0104 D4a), from
the build's `zephyr_modules.txt` at the manifest revision.

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
