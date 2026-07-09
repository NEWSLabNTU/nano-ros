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
