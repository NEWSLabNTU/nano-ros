# Phase 485 — dependency pinning: one pin set, one project lock

**Status (2026-10-11). M1–M3 measured, and the issue-1781 fix landed. W1 has
landed (the registry pin set), and so have W2 (seeded leaf locks) and W3 (the project lock across kinds); W4–W10 are open.** Implements
[RFC-0104](../design/0104-dependency-pinning-and-project-lock.md). The RFC's
D3 and D5 rested on facts that were not yet measured, so M1–M3 ran before any
implementation and amended the RFC.

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
[issue 1780](../issues/archived/1780-nros-build-keeps-generated-code-after-ament-change.md)
— W5's acceptance. Getting there found
[issue 1781](../issues/archived/1781-ament-index-last-prefix-wins.md): the CLI's ament
index lets the LAST `AMENT_PREFIX_PATH` entry win, so an overlay's interface
package is silently replaced by the underlay's — the overlay had to be put
LAST for the experiment to see it at all. The cmake and west roads were
measured with W5 (2026-10-11): their row B was already green whenever a
configure ran (the resolved interface PATHS are configure inputs), and their
live defect was the in-place `.deb`-mtime shape — see issue 1780's Resolution.

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

**Result (2026-10-11) — landed.**

- **`locks/cargo-registry.lock`** holds 512 pins, one per semver-compatible
  range, in cargo's own lock format. 420 of them come from the two locked
  roots.
  - It is named by `[rust] registry_pins`, beside `[rust] locked_roots = [".",
    "packages/cli"]`.
  - It is generated by `scripts/gen-cargo-registry-pins.py`, and
    `just lock-update` now runs that generator.
  - A pin comes from the first of these that holds the range: the locked
    roots, then the committed pin file, then the highest version any other
    tracked lock holds.
- **The two locked roots disagreed in 49 ranges** (`cc`, `clap`, `libc`,
  `syn`, `serde_json`, `wasm-bindgen`, …). Each lower one was moved up to the
  higher with `cargo update -p <name>@<old> --precise <new>`. Neither lock
  gained or lost a crate the root did not already carry.
  - Two moves were walked back because they pulled a new major in for
    nothing: `wasm-bindgen` 0.2.129 and `futures` 0.3.34 each require
    `syn` 3. Both sit at the root's earlier versions (0.2.122, 0.3.32), and
    no lock carries `syn` 3.
- **Seeding re-checked against the normalised file** (M1 had used the raw
  union). Two leaves were tested, `native/rust/logging` and
  `native/rust/talker`. A copy of the pin file followed by `cargo metadata`
  resolves every registry crate AT its pin, 25 of 25 and 90 of 90. A
  dependency string naming a version the pin set dropped does not upset
  cargo.
- **Gate `check-cargo-registry-pins`** (fast line, ~0.2 s) checks three
  things: one pin per range, each locked root a subset, and the pin file
  current. Its self-test plants a mismatch, an unpinned crate, another major
  and a duplicate. Run against a real planted `heapless` 0.8.1 in the root
  lock, it fails naming the crate.
- **The second gate (submodule-mode XOR clone-mode) is withdrawn.** Phase-484
  W3a (RFC-0103 D5) put a derived `ref` on all 14 submodule rows after M1–M3
  were measured. `check-source-refs` already holds that `ref` equal to the
  gitlink. One pin value is therefore met by derivation, and an exclusive
  gate would refuse what RFC-0103 requires. RFC-0104 D2 is amended
  accordingly.

### W2 — seeded leaf locks (D3)

`nros sync` / `nros build` seed each leaf `Cargo.lock` from the pin set and
resolve through cargo. Delete the tracked leaf locks under `packages/` that the
seed replaces; retire the `check-leaf-lockfiles` tracked-lock invariant.

**Acceptance:** every example leaf builds from a seeded lock on a fresh clone;
two hosts with different ament installs produce identical registry sets.

**Result (2026-10-11) — landed.**

- **One seeding function, `orchestration::registry_pins::seed`.** It copies
  the pin set in as `<root>/Cargo.lock` when the root has none, and never
  touches an existing lock: that is either a previous seed or a deliberate
  move, and judging it is the drift check's job (W4). Cargo then prunes the
  copy to the root's graph.
- **`nros build` seeds at all three places it can create a lock:** a
  generated entry or lock-less workspace root, a native cargo-member package,
  and the single-package leaf road. The leaf road now resolves once before
  building, as the other two already did, because `--frozen` cannot prune a
  seed.
- **`nros ws seed-lock <dir>`** is the same function for an invoker that runs
  cargo itself. It asks `cargo locate-project --workspace` first and leaves a
  workspace MEMBER alone, since a lock beside a member is never read. Its one
  shell spelling is `nros_seed_leaf_lock` in `scripts/build/cargo.sh`. That
  helper is called by:
  - the fixture builder, once per row directory, in the parent before make
    fans out;
  - the staleness probe (`rust-fixture-stale.sh`), which may be the first
    cargo to touch a row;
  - every staged root in `compile-check-fixtures.sh`;
  - the five just recipes that run `cargo build` inside a leaf (`native`,
    `freertos`, `qemu-baremetal` ×2, `ros-editions` ×2).

  The helper is never fatal: with no CLI, or a manifest cargo cannot load,
  the next cargo command resolves or reports as before.
- **The tracked-lock invariant is now "tracked ⟺ the directory is in
  `[rust] locked_roots`"**, held by `check-cargo-registry-pins`. The old
  invariant in `check-leaf-lockfiles` ("no message deps, or a committed
  `generated/`") is retired. That gate keeps its other half: each tracked
  lock still satisfies its own manifest.
- **15 tracked locks deleted:**
  - the 13 fixture-row roots (12 `nros-tests/bins/*` and
    `qemu-smoltcp-bridge`), which the seed now reaches;
  - `colcon-cargo-ros2/Cargo.lock`, which was dead: the crate is a member of
    the CLI workspace, so cargo never read it.

  The bins' `.gitignore` is a blanket `/*/Cargo.lock` now. It had been a
  list only because some bins kept tracked locks.
- **22 roots stay tracked, as locked roots, each normalised to the pins:**
  - `nros-launch-resolve` (a shipped tool);
  - the 10 board crates, 4 drivers, 2 `stm32f4-porting` references,
    `zpico-serial`, 2 smoke crates, `nros-verification`, and the colcon
    sample package.

  These are built standalone by a dozen scripts and workflows that call
  cargo by hand (nightly, nuttx, the arm-compile checks). The seed does not
  reach them, so W2's "delete what the seed replaces" does not cover them.
  Tracked and subset-gated, they can no longer disagree with the pins, which
  was the defect.
  - Normalising them dropped `syn` 1 from 15 locks and swapped
    `proc-macro-error` for `proc-macro-error2` in four, both following the
    root's versions.
  - `cargo check` passes on the stm32f4 references, the smoke echo, two
    boards and `nros-launch-resolve`.
  - `nros-verification` needs Verus (`spec fn`) and fails plain `cargo
    check` before and after, so that is not a regression.
- **Measured.** `examples/native/rust/talker` with its lock removed builds
  through `nros build` with 90 of 90 registry crates at their pins. The
  fixture bin `rosout-talker`, seeded by the shell helper and resolved by
  `cargo metadata`, has 105 of 105 at their pins. The same bin resolved
  UNSEEDED has 29 crates off pin (`cc` 1.7.0 against 1.2.63, `libc`
  0.2.190 against 0.2.186, `zerocopy` 0.8.62 against 0.8.50, …). That gap
  is the one RFC-0104 exists to close.
- **Not measured: two hosts with different ament installs.** There is one
  host here, as in M1. Generated crates are `0.0.0` path dependencies, so the
  registry set is host-independent by construction.

### W3 — the project lock across kinds (D4)

Extend `SdkLock` (`nros-sdk.lock`) with `[source.*]` (commit + origin, from
M3), `[cargo]` (registry set), `[system.*]`, `[generated.*]`. Written on every
build, only when content changes (the existing write-if-changed).

W3 also records `[west.<module>]` for a Zephyr image (RFC-0104 D4a), from
the build's `zephyr_modules.txt` at the manifest revision.

**Result (2026-10-11) — landed.** `[system.*]` belongs to W6 and
`[generated.*]` to W5; each is written by the work item that reads it.

- **What gets recorded.** `orchestration::project_lock::record` folds each
  finished plan into the lock that `store::lock_path_for` finds from the
  project. That is the same lock `nros setup` writes `[tool.*]` into. It adds
  three tables:
  - `[source.<name>]`: commit, origin rung, and `dirty` when set;
  - `[west.<module>]`: revision;
  - `[cargo."<root>"]`: `sha256:` over the registry set, plus a count.

  Entries merge, so a project with several images keeps every image's
  sources. The file is written only when its content changes: a second
  identical build writes nothing.
- **Derived from what the build read, never from what the index declares.**
  The input is every path in the build's own records:
  - cargo dep-info (`*.d`, continuation lines included);
  - build-script `rerun-if-changed` from `build/*/output`;
  - `ninja -t deps` wherever the walk meets a `build.ninja`;
  - Zephyr's `zephyr_modules.txt`.

  A `[source.*]` is recorded iff a read path lies under its located root (the
  same `locate` ladder `resolved.toml`'s `[locations]` uses). An unreadable
  tree is a smaller record, never a failed build.
- **Scoped to THIS image's directories.** The first version scanned the
  whole `<ws>/build/`. It then recorded a native Cyclone image as reading
  FreeRTOS, ThreadX, lwIP, NetX Duo and XRCE, all from other images' build
  trees. Each driver arm now names its own directories:
  - the generated entry's `target/`;
  - `build/<coord>/` for cmake;
  - the west `-d` directory.
- **Measured, on the native talker (cargo) and two workspace cmake images.**
  - **Talker and C `native` (zenoh, cmake).** Both record
    `[source.zenoh-pico]` at the pin, from the checkout. For the cmake image
    that comes through its cargo half, as M3 found. `mbedtls`, declared and
    not read, is absent, as M3 measured.
  - **C `native_cyclonedds`.** Records no source at all, and that is correct.
    Its Cyclone is the TOOL store's prebuilt
    (`~/.nros/sdk/cyclonedds/0.10.5-nros1`, which `dds/dds.h` resolves to),
    not `[source.cyclonedds-src]`. That corrects M3's row for this image.
  - **West.** Covered by a unit test over the real file format; a parse of a
    real `zephyr_modules.txt` from a 3.7 build has the same shape. No west
    image was built live for this item.
- **RFC-0065 D1 amended.** The handoff used to be `exec`, so nothing could
  run after the build. Every plan is now waited on with stdio INHERITED.
  Nothing is captured, and a failing child's code (or 128 + signal) is
  `nros build`'s exit code, carried as `ChildExit`. The two guarantees `exec`
  was chosen for are kept, and the record has a place to happen.
- **RFC-0104 open question 3 answered: a hash.** The leaf `Cargo.lock`
  beside the root is the diffable set. `[cargo]` is keyed relative to the
  LOCK, not the project, because a checkout's leaves all record into the one
  lock at its root, where a project-relative key would be `"."` for all of
  them.

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

**Done (2026-10-11, issue 1780 resolved).** The record is
`<generated pkg>/nros-generated.toml`, written by every generator
(`rosidl_codegen::ament_stamp`, one hash: `package.xml` + interface files over
the `<depend>` closure, path-independent). `nros build` regenerates a moved
cargo-road crate before planning; both cmake generators ask
`nros codegen --ament-witness` at configure and compare by content
(`nros_codegen_ament_stale`). Measured on all three roads, including the
in-place `.deb`-mtime shape that the cmake (Unix Makefiles) and west lanes
missed even though their row B was already green — table in the issue. W3 still
owes copying the per-package records into the project lock.

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
