# RFC-0104 — Dependency pinning: one pin set, one project lock, every ecosystem

**Status:** Draft (2026-10-10)

Implemented by [phase-485](../roadmap/phase-485-dependency-pinning.md). Builds on
the dependency SSoT of [RFC-0062](0062-unified-dependency-ssot.md) and
[RFC-0095](0095-nros-store-is-the-root.md) D9 ("pin on first build"); amends the
`--locked` shim of issues 0359/0378 and the tracked-lock invariant
`check-leaf-lockfiles` enforces. Composes with [RFC-0098](0098-generated-leaf-build-config.md)
(the generated per-image cargo config is where a seeded leaf lock is consumed).

## Summary

A lockfile exists so that a CLIENT's build resolves what ours did. Today only
the Rust half of a nano-ros image has one, it is scattered over ~40 tracked
`Cargo.lock` files that cover none of the 112 example manifests, and it fights
the half of the graph that is GENERATED on the host. This RFC splits every
dependency into what is PINNED (one reviewed set, owned by the index) and what
is LOCAL (produced on the host, never pinned), records what one build actually
used in ONE per-project lock across Rust crates, C/C++ sources, tools and
system libraries, and gives a developer one verb to move a pin.

## Motivation / problem

**The record.** Thirteen issues are about lockfiles, in two classes:

- **Enforcement leaks** (0384, 0386, 0676, 0873, 1307, 1356, 1358): the
  `scripts/bin/cargo` PATH shim injecting `--locked` put the flag after `--`,
  skipped it under `--offline`, missed nested cargo invocations and looked for
  the lock in the manifest dir; offline CI met cold registry caches. None is
  about the policy — each is a new way to invoke cargo that the shim did not
  see.
- **The policy fought the generated graph** (0012, 0066, 0359, 0394, 1182, 1184,
  1398): generated message crates carried the ament package version, so a
  tracked lock asserted WHICH ROS install built it; tracked locks needed
  `nros sync` to have run; the lock gate skipped exactly the trees that drifted.

**The gap nobody filed.** `git ls-files 'examples/**/Cargo.lock'` is empty
against 112 example manifests. Every user who copies an example, and every CI
build of one, resolves the registry fresh — the consistency a lock exists to
give is absent exactly where a client starts.

**A lock is not only Rust.** An image is built from seven kinds of dependency,
and the C/C++ half already has its pin set — in the index — but no per-project
record:

| kind | example | pinned by | recorded per project |
| --- | --- | --- | --- |
| Rust registry crates | heapless, serde, esp-hal | tracked `Cargo.lock` (38 roots) | the leaf `Cargo.lock` |
| vendored C/C++ sources (20 submodules, 15 `[source.*]`) | zenoh-pico, cyclonedds, mbedtls, FreeRTOS, lwip | the GITLINK (14 submodule-mode sources carry no `ref`); `ref` for the one clone-mode source (`rosidl`); data in an installed SDK root (issue 1304) | **nothing** |
| tools (41 `[tool.*]`) | arm-gcc, ninja, idlc, corrosion | index `version` + `sha256` | `nros-sdk.lock`, tools only |
| west modules (Zephyr) | zephyr, `lang-rust`, cmsis, picolibc, mbedtls, mcuboot, tinycrypt | `west.yml` / `west-4.4.yml` revisions — the module SET is derived from `[zephyr_module.*]` (phase-447 F1), the REVISIONS are not in the index | nothing |
| fetched at build | Corrosion's `FetchContent` fallback | a cmake tag | nothing |
| system / ROS | `rmw_zenoh_cpp`, ament msg packages, glibc | not ours (a measured floor — RFC-0099 D5) | nothing |
| generated | msg crates, C/C++ msg packages, entries | codegen version + the host's ament inputs | the codegen-version guard |

## Design

### D1 — Two halves: PINNED and LOCAL

Every node in an image's dependency graph is either **pinned** (identical on
every host; one reviewed value) or **local** (produced from the host; never
pinned, always regenerated). Pinned: registry crates, vendored sources, tools.
Local: generated code of every language (Rust msg crates, C/C++ msg packages,
generated entries), `[patch]` rows into generated trees, path deps. System
libraries are neither — they are the host's (D6).

Every lock defect in the record is a tracked file asserting a LOCAL fact, or an
enforcement trying to freeze both halves at once. The split is the fix.

### D2 — One pin set, owned by the index

`nros-sdk-index.toml` is already the pin set for every non-Rust kind (RFC-0062).
Rust registry pins join it: the index names ONE tracked, registry-only lock —
`[rust] registry_pins = "locks/cargo-registry.lock"` — the UNION of what every
root resolves (the repo root, the CLI workspace, every leaf). It is the only
reviewed lockfile diff in the tree.

The tracked root and CLI `Cargo.lock`s (closed graphs) stay, and are checked to
be SUBSETS of the pin set, so `heapless` cannot be 0.8.1 in the root and 0.8.0
in the examples.

A vendored source has ONE pin value: the gitlink for a submodule-mode source,
`ref` for a clone-mode one — already true (measured 2026-10-10: no source has
both). The update verb (D7) writes whichever the entry uses; a gate keeps the
two modes exclusive.

### D3 — Leaf locks are SEEDED, never tracked

A leaf `Cargo.lock` is written by `nros sync` / `nros build` from the pin set
(the registry entries, copied whole, checksums included), then cargo resolves —
keeping every seeded version the manifests still accept, pruning the rest, and
adding the LOCAL path crates (deterministic: generated crates are `version =
"0.0.0"` since issue 0394). Cargo does the resolution (D8); `nros` only decides
the starting point. **Precondition, to be measured (phase-485 M1):** cargo
keeps seeded versions and prunes unused entries — including entries for targets
the leaf does not build — when a lock is a superset of the graph.

### D4 — One project lock across every kind

`nros-sdk.lock` grows from tools to everything a build used:

```toml
[tool.arm-none-eabi-gcc]     # as today: version + provenance + sha256
[source.zenoh-pico]          # the commit actually checked out, and from where
commit = "e28ff60…"
[west.picolibc]              # a Zephyr image: each module the BUILD used
revision = "…"               # (zephyr_modules.txt), at its west revision
[cargo]                      # the leaf lock's REGISTRY set, by hash; the
registry = "sha256:…"        # leaf Cargo.lock itself stays beside the leaf
[system.rmw_zenoh_cpp]       # OBSERVED, never enforced (D6)
version = "0.1.9"
[generated.std_msgs]         # what the local half came FROM (D5)
ament = "sha256:…"           # hash of the ament inputs; codegen version
codegen = "…"
```

In a checkout it is a build artifact (gitignored, as today). In a USER project
it is written on the first `nros build` (RFC-0095 D9) and is the user's to
commit; `nros` never rewrites a pinned entry without `nros update` (D7).

**`[source]` and `[west]` are DERIVED from what the build read, never listed**
(phase-485 M3). An image's set is the union of its ninja dependency records
and its cargo half's build-script `rerun-if-changed` + dep-info — a cmake
image's vendored RMW C is compiled by the cargo half, so ninja alone sees
none of zenoh-pico or micro-XRCE — plus, for a Zephyr image, the build's own
`zephyr_modules.txt`. Declared is not used: `mbedtls` is in the index and no
measured image read it.

### D4a — West modules are a pinned kind of their own

A Zephyr image also builds from west modules (the kernel, `lang-rust`, HALs,
picolibc, mbedtls, mcuboot …). `west.yml` pins their REVISIONS; the index
derives only the module SET (`[zephyr_module.*]`, `check-zephyr-module-allowlist`).
They are pinned — identical on every host — so they belong to D1's pinned
half, recorded as `[west.<module>]` from the build's `zephyr_modules.txt` at
the manifest revision, and checked by D9 like a source. `nros update --west
<module> --revision <rev>` (D7) edits the manifest. Whether the revisions
move INTO the index (so the manifests are fully derived, which RFC-0099 D10
points toward) is open question 5.

### D5 — The local half regenerates when its host inputs move

Recording the generated half is not enough: when the ament inputs a generated
package came from change (`apt install --only-upgrade 'ros-humble-*'`), the
build must regenerate it before compiling, or the image carries generated code
that no longer matches its ROS peers — 1018's freshness class with the ROS
install as the moved input. `[generated.<pkg>] ament` is the key: a mismatch
re-runs generation for that package. **Today's behaviour is unmeasured**
(phase-485 M2) and is the most likely live defect here.

### D6 — System libraries: RECORD and WARN, never enforce

The build system asks one question of an environment (issue 1248); pinning the
host's ROS would encode it. So `[system.*]` records what was observed and a
build that sees a change WARNS, once, naming what moved and what it touched:

```
nano-ros: system changed since the last build (nros-sdk.lock):
  ros-humble-std-msgs   4.2.3 -> 4.2.4   regenerated: std_msgs
  rmw_zenoh_cpp         0.1.1 -> 0.1.9   its zenoh 1.2.0 -> 1.8.0
```

The warning is only useful against a declared range: the index states
COMPATIBILITY RANGES for the system pieces a pinned piece must interoperate
with (e.g. zenoh-pico 1.8 ↔ zenoh `>=1.0,<2`), and an observation outside one
warns loudly, naming the range (issue 0609's drift, caught at build time).

### D7 — One update verb, per kind

```
nros update --crate <name> [--precise <v>]
nros update --source <name> --ref <sha>
nros update --tool <name> --version <v>
nros update --west <module> --revision <rev>
```

Each edits the pin set (the registry pin file, the gitlink or `ref`, the index
`[tool.*]`), re-seeds, re-records, and leaves ONE diff to review. `--crate`
resolves every root that uses the crate (`cargo update -p --precise` per root),
merges into the pin set and updates the tracked root/CLI locks in the same
step. A breaking bump (`0.8` -> `0.9`) is a manifest edit first — `nros update`
may call `cargo upgrade` (D8) per root when present, and names it as the remedy
when not — and then the same verb; a root still resolving the old major is
REPORTED. Multiple majors of one crate are ALLOWED and REPORTED (transitive deps
make some unavoidable).

### D8 — `nros` owns the contract; cargo owns resolution; `cargo-*` are developer-only

- **`nros`**: the pin set, seeding, the project lock, the drift check, the
  update verb, regeneration (D5), the system warning (D6). Only `nros` reads
  the index, so only it can compare a lock across kinds — and a user already
  has it. Per `packages/cli/CLAUDE.md`, the pin file's LOCATION comes from the
  index, never from the CLI.
- **cargo itself, never reimplemented**: `cargo update -p --precise` (moving a
  pin), `cargo metadata` (what a root resolved; driving a seeded resolve),
  `cargo tree -d` (duplicates); the `cargo-lock` crate for lock structure — a
  hand-written resolver or lock parser is the copy that drifts.
- **`cargo-*` add-ons, developer-only, provisioned as `[tool.*]` / `just
  dev-tools`, never reached from a user build**: `cargo-deny` (duplicates,
  RUSTSEC advisories, licences, banned sources over the pin set),
  `cargo-edit`'s `cargo upgrade` (the breaking bump across manifests),
  `cargo vendor` (an offline CI mirror — the 0873/1356 cold-cache class).

### D9 — One drift check replaces three mechanisms

The project lock against the pin set, kind by kind: a registry crate outside
the pins, a source at another commit, a tool whose sha256 differs, a tracked
root lock that is not a subset. It is a SET comparison after resolution, so
there is no invocation path to bypass. It replaces the shim's leaf role,
`check-leaf-lockfiles`, and the client side of the submodule-pin checks.

`--locked` survives only for the two closed-graph roots (repo root, CLI), moved
from the PATH shim into the recipes that build them; the shim retires. (The
pre-push `check-submodule-pins` FORWARD-ONLY rule is about history, not drift,
and stays.)

### D10 — No fetch at build time

Every tool comes from the store. Corrosion's `FetchContent` fallback in
`cmake/NanoRosCorrosion.cmake` is phased out: a configure that cannot find the
pinned Corrosion REFUSES and names `nros setup` (RFC-0065 D2's
refuse-and-name-the-remedy), so no pin goes unrecorded.

## Walkthroughs

**A user upgrades ROS and rebuilds** (`apt install --only-upgrade
'ros-humble-*'`, then `nros build`). Pins do not move; the seeded leaf
`Cargo.lock` does not move (generated crates are `0.0.0`; a msg package that
gained a msg dependency is a LOCAL change). `[generated.*] ament` hashes differ,
so the affected packages regenerate (D5). The build warns once with what moved,
loudly if outside a declared range (D6). If the user commits `nros-sdk.lock`,
the diff is the `[system]`/`[generated]` lines — exactly the upgrade. The user
types nothing extra.

**A developer bumps heapless.** Compatible: `nros update --crate heapless
--precise 0.8.1` — every root that uses it re-resolves, the pin set and the root
/ CLI locks move together, one diff, then the drift check and `just ci gate`.
Breaking: edit manifests (or `cargo upgrade`), the same verb, stragglers
reported. Released users get the new pins with the next SDK release; their own
lock does not move until THEY run `nros update`.

## Alternatives considered

- **Status quo, hardened** — examples stay unpinned, and the shim keeps a leak
  surface that grows with every new way of invoking cargo.
- **Track every leaf lock** — a tracked file cannot hold a per-host graph
  (issues 0386, 0394).
- **Exact `=` pins in manifests** — pins direct deps only, not transitive; 112
  standalone leaves share no workspace to centralise them.
- **Vendor / mirror the registry as THE strategy** — full reproducibility and
  offline, but heavy, and it does nothing for the generated half. Kept as a
  developer/CI complement (D8).
- **`cargo-hakari`** — solves feature unification inside one workspace; the
  leaves are deliberately standalone.
- **Dependabot** — cannot read the pin file or gitlinks. Renovate with custom
  managers is a possible later bot, not part of the design.
- **Pin system libraries** — encodes the environment (issue 1248); record and
  warn instead (D6).

## Open questions

1. Where the registry pin file lives and its format — a plain registry-only
   `Cargo.lock` (cargo can read it as a seed directly) vs a TOML subset keyed
   by name; M1 decides.
2. The compatibility-range vocabulary in the index (D6): per `[source.*]`, per
   `[rmw.*]`, or a separate `[compat.*]` table.
3. Whether `[cargo]` in the project lock stores a hash or the registry set
   itself — a hash is small, the set is diffable.
4. How a released SDK ships the pin set beside the index (RFC-0099 D2's SDK
   root) and how `nros update` behaves when the index is an installed one
   (read-only: an `--override` file in the project?).

5. Whether west module REVISIONS move into the index (`[zephyr_module.*]
   revision`), making `west.yml` fully derived (RFC-0099 D10), or stay in the
   manifests with the lock recording them (D4a).

## Changelog

- 2026-10-10 — created (Draft) from a maintainer design discussion: D6 record
  and warn, D10 retire the Corrosion fetch, D2 one pin value per source, D8 the
  nros/cargo/cargo-* split.
- 2026-10-10 — D4a: west modules are a seventh dependency kind, pinned by
  `west.yml`, recorded per image from `zephyr_modules.txt` (phase-485 M3); the
  project lock's `[source]`/`[west]` sections are derived from build records;
  open question 5.
