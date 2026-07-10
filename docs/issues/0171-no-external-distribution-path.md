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
