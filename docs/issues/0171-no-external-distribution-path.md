---
id: 171
title: "No external integration path avoids vendoring the full monorepo — CLI/crates unpublished, no CMake package, registry publishes docs-only, false availability claims"
status: open
type: enhancement
area: build
related: [rfc-0003, rfc-0014, rfc-0040, phase-140, phase-222]
---

## Problem

A user who wants nano-ros **in their own RTOS project** (not the in-repo
examples) cannot do it without cloning and building the whole monorepo, on
every surface:

| Surface | State | Blocker |
| --- | --- | --- |
| `nros` CLI | per-checkout build only (`just setup-cli`); `publish = false`; global install called a footgun (`book/src/internals/cli-in-monorepo.md`) | gates every other path at step 0 |
| Rust crates | every runtime crate `publish = false`; consumers vendor + `[patch.crates-io]` + `NROS_REPO_DIR` (RFC-0040, `cargo-nano-ros/src/scaffold.rs`) | no `cargo add nros` |
| C/C++ CMake | `add_subdirectory(<repo-root>)` is the only contract; `find_package(NanoRos)` + all `install()` rules removed in Phase 140 (`docs/reference/c-api-cmake.md`) | whole tree submoduled, Rust core rebuilt per project |
| Zephyr west module | best story — real module + Kconfig — but module root = repo root | `west update` clones the monorepo |
| ESP-IDF | `integrations/nano-ros/idf_component.yml` works via path/git | registry publish is docs-only, never executed, no CI (`docs/release/registry-publishing.md`) |
| PlatformIO | root `library.json` + codegen hook | unpublished; `registry-publishing.md` references `integrations/platformio/library.json` + `library.properties` that **do not exist** |
| Arduino | claimed in `library.json` `frameworks` | nothing implemented — no `library.properties`, no glue |
| FreeRTOS / ThreadX | in-tree boards only | no bring-your-own-board doc distinct from contributor pages |

**False availability claims** (fix these immediately, independent of the
distribution decision):

- `packages/cli/{nros-cli,cargo-nano-ros,nros-cli-core}/README.md` claim
  `cargo install nros-cli` + crates.io links — contradicted by
  `publish = false`.
- `library.json` advertises `"arduino"`.
- `registry-publishing.md` points at nonexistent PlatformIO manifest paths.

> **Planned.** The reshape is split into two phase docs:
> [phase-287](../roadmap/phase-287-cmake-consumption-reshape.md) (C/C++ CMake
> one-line bootstrap + example migration, #171 D5) and
> [phase-288](../roadmap/phase-288-source-distribution-bootstrap.md)
> (source-distribution bootstrap: one front door, fix the false prebuilt claim,
> #171 D1/D2). Priority 1 (false claims) already landed; D3/D4/D6/D7 are decided
> (see the decision log below).

## Fix direction

Priority order:

1. Truth-fix the false claims now (remove `cargo install`, crates.io links,
   Arduino from `library.json`, fix `registry-publishing.md` paths).
2. Ship the `nros` CLI as an installable artifact (prebuilt release binaries
   via nano-ros-sdk Releases and/or a genuinely published crate) — the single
   highest-leverage unlock.
3. Publish the runtime crates (or make `NROS_REPO_DIR` auto-provision).
4. Provide a real CMake consumption story (`find_package` config + install, or
   a pinned FetchContent recipe) for external C/C++ users.
5. Execute + CI-automate the ESP-IDF / PlatformIO registry publishes (manifests
   are done; only the last 10% is missing).
6. Add greenfield bring-your-own-project/board docs for FreeRTOS, ThreadX,
   baremetal.

## Priority 1 — false availability claims removed (2026-07-10)

Truth-fixed, independent of the distribution decision. Every claim was verified
against the tree before editing:

- **`cargo install nros-cli` / crates.io links.** All CLI crates are
  `publish = false` (`nros-cli`, `cargo-nano-ros`, `nros-cli-core`), and so is
  every runtime crate — *nothing* is on crates.io. `nros-cli/README.md` and
  `cargo-nano-ros/README.md` no longer print a `cargo install` line; they give
  the real route (`git clone` → `just setup-cli` → `source activate.sh`, which
  builds `packages/cli/target/release/nros` and puts it on `PATH`) and point at
  `book/src/internals/cli-in-monorepo.md` for why a *global* `nros` is a
  footgun. `nros-cli-core/README.md`'s `crates.io/crates/nros-cli` link now
  points at the sibling crate.
- **`book/src/user-guide/logging.md`** linked `crates.io/crates/nros-log`;
  `nros-log` is `publish = false` too. Now links the in-tree crate and says so.
- **`library.json` advertised `"arduino"`** with nothing behind it (no
  `library.properties`, no glue — only a research note and an archived phase).
  Removed from `frameworks`; `zephyr` + `espidf` stay (they have a real
  `zephyr/module.yml` and `integrations/nano-ros/idf_component.yml`).
- **`docs/release/registry-publishing.md`** pointed at
  `integrations/platformio/library.{json,properties}` — neither exists. The PIO
  manifest is `library.json` at the **repo root** (`integrations/platformio/`
  holds only `nros_codegen.py`). Paths fixed, the publish command corrected to
  run from the root, and the section now states plainly that the publish has
  **never been executed and has no CI**.
- **`docs/release/migration-install-local-removal.md`** told users to add
  `lib_deps = nano-ros@*`; the library is unpublished, so that never resolves.
  Corrected to a path/git pointer.

Verified: `library.json` still parses as JSON; every replacement link target
exists; `mdbook build` clean. (`just book` fails on an unrelated pre-existing
regression — its `cargo doc --features rmw-zenoh,…` selects packages that no
longer carry that feature, likely phase-248 C6e fallout.)

**Still open: priorities 2–6.** Those need a *distribution decision* that is a
product/release-policy call, not a mechanical fix: whether to publish the CLI
(crates.io vs prebuilt release binaries), whether to publish the runtime crates
or auto-provision `NROS_REPO_DIR`, whether to restore `find_package(NanoRos)` +
`install()` rules retired in Phase 140, and whether to actually execute + CI the
ESP-IDF / PlatformIO registry publishes.

## Distribution decisions (2026-07-10/11)

Agreed in discussion. **Actions are compiled into a separate phase doc**; only
the two book fixes below shipped here.

- **D1 — CLI via a bootstrap SCRIPT that BUILDS from source.** Not a `just`
  step (fresh env may lack `just`), not folded into `nros setup` (needs the
  binary first). `scripts/bootstrap.sh` already builds from a checkout without
  `just`. *Action (phase):* make it the one front door; `packages/cli/
  install.sh` advertises a **prebuilt** download from a `NEWSLabNTU/nros-cli`
  Releases repo that (per this issue) does not exist — a 4th false claim to
  fix.
- **D2 — bundled source; no crates.io, no prebuilt libraries.** Mixed-language
  runtime (no_std Rust + C/C++ FFI) × many platform/arch combos makes prebuilt
  infeasible and crates.io unable to carry the C/C++ deps. Consumption model:
  user pulls the nano-ros source at a pinned version → runs the bootstrap
  (builds CLI) + `nros setup <board>` (prereqs) → their project's manifest
  points at the nano-ros **entry manifest** (CMake include into the checkout;
  Cargo `[patch.crates-io]` → checkout). Supersedes priority 3.
- **D3 — no Arduino.** Done (removed from `library.json`, priority 1).
- **D4 — no PlatformIO for now (future work).** Manifest + extra-script stay
  in-tree but unpublished. Narrows priority 5 to ESP-IDF (also unexecuted / no
  CI). Not mentioned in the book (D7).
- **D5 — C/C++ per-package self-contained, boilerplate simplified.** A leaf
  already builds standalone via the `NANO_ROS_ROOT` guard, but every leaf
  copy-pastes the ~10-line guard + the `if(NROS_RMW STREQUAL cyclonedds)
  enable_language(CXX)` micro-option. *Action (phase):* the CMakeLists must be
  **identical for any RMW/platform** — collapse to a one-line
  `nano_ros_bootstrap()` + `nano_ros_entry()` + link; hide the CXX/RMW knobs.
  No `find_package`/`install` (Phase 140 removed them; source-tree include fits
  D2). **A standalone phase reshapes the CMake and migrates all examples.**
- **D6 — bring-your-own-board docs already exist.** `concepts/board-
  integration.md` is a user-profile→path matrix (FreeRTOS/ThreadX/NuttX/
  bare-metal/Zephyr/ESP-IDF/PX4/niche-fork); `porting/{board-crate-import,
  custom-board,vendor-overlay}.md` cover consuming/writing one. *Done here:* a
  brief cross-link from `getting-started/freertos.md` → the matrix. Priority 6
  otherwise de-scoped.
- **D7 — the book (`book/src/`) never discusses publish status or future
  work** (it is user-facing). *Done here:* reverted the publish aside my
  priority-1 edit added to `user-guide/logging.md`. (`publish = false` inside
  Cargo.toml *snippets* and the contributor `internals/creating-examples.md`
  explanation of the `[patch.crates-io]` mechanism are fine — not user-facing
  availability claims.)

### Cargo consumption UX (open, phase)

`[patch.crates-io]` works but is heavy on the consumer side. Evaluate lighter
handles (path deps via a generated `.cargo/config.toml`, a `[patch]` on a git
source, a workspace inherit, or a `nros sync`-managed block) and pick the best
UX. Decide in the CMake/consumption reshape phase.
