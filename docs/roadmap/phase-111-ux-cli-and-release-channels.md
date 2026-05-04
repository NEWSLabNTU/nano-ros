# Phase 111: `nros` CLI + Multi-Channel Release Pipeline

**Goal:** Ship a single `nros` command-line utility as the canonical user entry point for nano-ros (scaffolding, message generation, configuration, build, run), and stand up a release pipeline that publishes Rust crates to crates.io and C/C++ libraries to per-platform channels (Arduino Library Registry, ESP-IDF Component Registry, PlatformIO, Zephyr west module, GitHub Releases tarball).

**Status:** Not Started
**Priority:** High
**Depends on:** Phase 23 (Arduino precompiled lib), Phase 75 (relocatable CMake install), Phase 78 (colcon-nano-ros), Phase 88 (`nros-log`)
**Related research:** `docs/research/sdk-ux/SYNTHESIS.md`, `docs/research/sdk-ux/{micro-ros,zephyr-and-esp-idf,platformio-arduino-mbed}.md`

---

## Overview

The cross-RTOS UX research (`docs/research/sdk-ux/`) compared nano-ros to micro-ROS, ESP-IDF, Zephyr `west`, PlatformIO, Arduino CLI, and Mbed CLI 2. Two convergent findings dominate:

1. **There is no user-facing CLI.** `cargo nano-ros` only reaches Rust users with cargo installed; `just` is a maintainer recipe surface (60+ recipes, internal stale-guards, install-local helpers). Customers on Arduino, PlatformIO, ESP-IDF, or any C/C++-only flow have nothing to type.
2. **There is one distribution channel.** "Clone the monorepo + `just setup`". Reference SDKs ship 5–10 channels (Arduino zip, IDF component, west module, PIO library, Docker agent images, …).

Phase 111 closes both gaps:

- **Pillar A — `nros` utility.** Standalone Rust binary, packaged for every host OS, with subcommands for scaffolding, codegen, config, build, run, doctor, and (maintainer-side) release. The existing `cargo nano-ros` subcommand becomes a thin alias that calls the same library crate.
- **Pillar B — Release pipeline.** Bump-detect-and-publish script for Rust crates (leaf-first, dependency-ordered, CI-enforced). Per-platform packagers for C/C++ deliverables, each with its own CI job, documented consumption snippet, and version-pinned to the same release tag as the Rust workspace.

Out-of-scope items deferred to later phases (tracked in `SYNTHESIS.md`): `nano-ros.toml` config-file unification (UX-40), package registry / `nros add` (UX-41), board descriptor TOML (UX-42), runtime transport vtable (UX-22).

---

## Architecture

### A. The `nros` binary

Crate layout — extend the existing `packages/codegen/packages/cargo-nano-ros/` workspace:

```
packages/codegen/packages/
├── cargo-nano-ros/          # existing — `cargo nano-ros <verb>` adapter
│   └── src/main.rs          #   becomes a thin shim that calls into nros-cli
├── nros-cli/                # NEW — the standalone `nros` binary
│   ├── Cargo.toml           #   [[bin]] name = "nros"
│   ├── src/main.rs          #   clap dispatch
│   └── src/cmd/
│       ├── new.rs           # scaffolder
│       ├── generate.rs      # message codegen front-end
│       ├── config.rs        # config inspection / validation
│       ├── build.rs         # cargo / cmake / west wrapper
│       ├── run.rs           # build + flash + monitor chained verb
│       ├── doctor.rs        # platform doctor consolidator
│       ├── board.rs         # `nros board list`
│       └── release/         # maintainer-only subcommands (gated by feature flag)
│           ├── detect.rs    # version-bump detection
│           └── publish.rs   # ordered crates.io publish
└── nros-cli-core/           # NEW — library crate, the actual logic
    └── src/lib.rs           # so cargo-nano-ros, nros, and tests share code
```

Verb surface (user-facing):

| Verb | Purpose | Notes |
|---|---|---|
| `nros new <name>` | Project scaffolder | `--platform <freertos\|nuttx\|threadx\|zephyr\|esp32\|posix\|baremetal>` `--rmw <zenoh\|xrce\|dds>` `--lang <rust\|c\|cpp>` `--use-case <talker\|listener\|service\|action>` |
| `nros generate <lang>` | Message bindings | Wraps existing `cargo nano-ros generate-{rust,c,cpp}`. `lang` ∈ `rust\|c\|cpp\|all`. Reads `package.xml`. |
| `nros config show` | Print resolved config | Reads `config.toml` + Kconfig (Zephyr) + Cargo features; emits unified view. |
| `nros config check` | Validate config | Catches mismatched RMW × platform × ROS edition. Catches missing `config.toml` keys. |
| `nros build` | Build current project | Detects flavor (cargo vs cmake vs west) from project tree. |
| `nros run` | Build + flash + monitor | `--env <env-name>` selects target. Falls back to single-target for projects with one target. |
| `nros monitor` | Attach to running target | Pretty-prints + decodes panics via ELF (defmt / addr2line). |
| `nros doctor` | Health check | Aggregates per-platform doctors. Optional `--platform <name>` to scope. |
| `nros board list` | Enumerate supported boards | Reads `packages/boards/` registry. |
| `nros version` | Print toolchain + lib versions | Useful for bug reports. |

Maintainer-only verbs (compiled in with `--features release`, hidden from `--help` for normal users):

| Verb | Purpose |
|---|---|
| `nros release detect` | Diff each crate's `version =` against the latest published version on crates.io; emit a topo-sorted publish plan. |
| `nros release publish` | Execute the plan (dry-run by default; `--execute` to publish). Idempotent. |
| `nros release tag` | Create a single git tag (`v<workspace-version>`) and push. |
| `nros release c-libs` | Build, package, and tag every C/C++ release artifact. |

### B. Release pipeline

Three pipelines, each triggered by a single signed git tag (`vX.Y.Z`):

**B.1 — Rust crates.io pipeline**

Algorithm for `nros release detect`:

1. Walk the workspace, parse every `Cargo.toml` with `cargo metadata`.
2. For each crate, query `crates.io` (`/api/v1/crates/<name>`) for the latest published version.
3. Compare to the workspace version. Crates whose version bumped → "to publish". Crates that didn't bump but have a *transitive* dep that bumped to a non-semver-compatible release → flagged as error (must bump).
4. Topo-sort the to-publish set by `[dependencies]` graph; leaves first.
5. Emit a `release-plan.json` to stdout (and to `target/release-plan.json`).

`nros release publish` consumes the plan and runs `cargo publish -p <crate>` in order, with a 30-second sleep between each (crates.io index propagation). Failure aborts the chain.

CI enforcement: a `release-version-check` GH Actions job runs `nros release detect --check` on every PR. It fails if any crate's source content changed without a version bump *and* one of its non-dev deps bumped to an incompatible version.

**B.2 — C/C++ release channels**

For each release channel below, ship: (a) packaging recipe, (b) CI publish job, (c) one-paragraph "how to consume" docs section.

| Channel | Artifact | Tooling | Doc target |
|---|---|---|---|
| Arduino Library Registry | `NanoROS-<ver>.zip` containing `library.properties` + `src/<arch>/libnros.a` + headers | `tools/release/arduino-pack.sh`, GH Actions matrix per arch | `book/src/getting-started/arduino.md` (new) |
| ESP-IDF Component Registry | `idf_component.yml` + headers + per-target `libnros.a` (`esp32`, `esp32c3`, `esp32s3`) | `compote component upload` | `book/src/getting-started/esp-idf.md` (new) |
| PlatformIO Library | `library.json` + same per-arch `.a` collection | `pio package publish` | `book/src/getting-started/platformio.md` (new) |
| Zephyr west module | tagged-release reference for `west.yml` | git tag only — module is the repo | `book/src/getting-started/zephyr.md` (update) |
| GitHub Releases tarball | `nano-ros-<ver>.tar.gz` with `find_package(NanoRos)`-installable layout | `cmake --install --prefix` + `tar` | `book/src/getting-started/c-cpp-from-source.md` (new) |
| Agent Docker images | `ghcr.io/newslabntu/nano-ros-zenoh-router:<ver>`, `…/nano-ros-xrce-agent:<ver>` | GH Actions buildx | mentioned in every getting-started page |

Channel-publish ordering: Rust → GH Releases tarball → Arduino zip + IDF component + PIO (parallel) → Docker images. Failure of one channel does not block others; CI emits per-channel pass/fail.

**B.3 — Pre-1.0 cadence**

While the workspace is < `1.0.0`, every crate's version stays in lockstep with the workspace version. After 1.0, crates can drift independently and `nros release detect` becomes load-bearing. Document this in `docs/development/release.md`.

### C. Migration path from `just` and `cargo nano-ros`

Both stay. `nros <verb>` is the documented user verb; `cargo nano-ros <verb>` aliases through the same library; `just` recipes that wrap user-flow operations call into `nros` for consistency. Internal `just` recipes (build matrices, CI orchestration) keep their current shape.

`book/src/getting-started/*.md` is rewritten so every snippet uses `nros …`. The `cargo nano-ros …` form is kept in a single "Cargo subcommand" reference page for users who already have cargo and prefer it.

---

## Work Items

### A — `nros` utility binary

- [ ] **111.A.1** — Create `nros-cli-core/` library crate with subcommand traits + dispatch.
- [ ] **111.A.2** — Create `nros-cli/` binary crate with `[[bin]] name = "nros"`. clap-based parser.
- [ ] **111.A.3** — Reshape `cargo-nano-ros/` to be a thin shim calling into `nros-cli-core`.
- [ ] **111.A.4** — Implement `nros new` scaffolder. Templates live under `templates/<lang>/<platform>/<use-case>/`. Each template variable-substituted (project name, RMW choice, board). Output validated by running `cargo build` (Rust) or `cmake -B build` (C/C++).
- [ ] **111.A.5** — Implement `nros generate <lang>`. Wraps existing `cargo_nano_ros::GenerateConfig` for parity.
- [ ] **111.A.6** — Implement `nros config show` / `nros config check`. Parses `config.toml` + reads Kconfig (`zephyr-build`'s `kconfig` parser if applicable) + introspects Cargo features via `cargo metadata`.
- [ ] **111.A.7** — Implement `nros doctor` consolidating per-platform doctors (`packages/scripts/doctor-*.sh` family). Returns non-zero if any default-platform doctor fails.
- [ ] **111.A.8** — Implement `nros board list`. Reads board crate manifests from `packages/boards/` and emits `name | chip | flash | ram | supported_rmw` table.
- [ ] **111.A.9** — Implement `nros build`. Detects project flavor (cargo / cmake / west) by file presence (`Cargo.toml` + `[lib]/staticlib`, `CMakeLists.txt`, `prj.conf`). Delegates to the right tool.
- [ ] **111.A.10** — Implement `nros run`. Build → flash → monitor chained loop. v1 supports POSIX (`./target/.../<bin>`), QEMU (reuse `nros-tests::qemu`), ESP32 (espflash). v2 adds OpenOCD ARM boards.
- [ ] **111.A.11** — Implement `nros monitor`. Reuse `defmt-print` for ARM RTT; `addr2line` panic decoder for QEMU semihosting; raw passthrough for ESP32 (espflash already monitors).
- [ ] **111.A.12** — Top-level `nros --help` curated; verbose `--help-all` lists release subcommands.
- [ ] **111.A.13** — Shell completions (`nros completions bash|zsh|fish|powershell`) generated by clap_complete.

**Files**:
- `packages/codegen/packages/nros-cli/` (new)
- `packages/codegen/packages/nros-cli-core/` (new)
- `packages/codegen/packages/cargo-nano-ros/src/main.rs` (refactor)
- `templates/` (new and reorganized — current `templates/` has 4 stub Cargo.toml files; replace with full template trees)
- `book/src/reference/cli.md` (new — the canonical `nros` reference page)
- `book/src/getting-started/your-first-project.md` (new — single 5-command quickstart)

### B — Rust crates.io publish

- [ ] **111.B.1** — Audit every crate in `packages/core/`, `packages/zpico/`, `packages/xrce/`, `packages/dds/`, `packages/codegen/`. List authors, license, descriptions, repository links — every crate Cargo.toml must have these for crates.io.
- [ ] **111.B.2** — Decide naming. Crates.io has no namespacing; reserve `nros`, `nros-core`, `nros-serdes`, `nros-rmw`, `nros-rmw-zenoh`, `nros-rmw-xrce`, `nros-rmw-dds`, `nros-node`, `nros-c`, `nros-cpp`, `zpico-sys`, `cargo-nano-ros`, `nros-cli` immediately. Verify each is unclaimed.
- [ ] **111.B.3** — Implement `nros release detect`. Plan format: `{ "to_publish": [{ "name": ..., "current": ..., "published": ..., "deps": [...] }, ...], "errors": [...] }`. Topo-sort verified by unit test against a known-good workspace.
- [ ] **111.B.4** — Implement `nros release publish` with `--dry-run` default. Calls `cargo publish -p <name> --dry-run`/`--no-verify` as appropriate; sleeps between crates.
- [ ] **111.B.5** — Implement `nros release tag` (creates `v<ver>`, requires clean tree, pushes to `origin`).
- [ ] **111.B.6** — GH Actions: `release-version-check` (PR-time guard, fails on missing bumps), `release-publish-rust` (tag-time, runs `nros release publish --execute`).
- [ ] **111.B.7** — Update `CONTRIBUTING.md` with the version-bump rule.
- [ ] **111.B.8** — First publish: a `0.1.0` initial release. Verify every example still builds against crates.io versions (eliminate `[patch.crates-io]` from example trees as part of acceptance).

**Files**:
- `packages/codegen/packages/nros-cli-core/src/release/{detect,publish,plan,tag}.rs` (new)
- `.github/workflows/release-version-check.yml` (new)
- `.github/workflows/release-publish-rust.yml` (new)
- `CONTRIBUTING.md` (update)
- `docs/development/release.md` (new)
- Every workspace `Cargo.toml` — fill `description`, `license`, `repository`, `homepage`, `documentation`, `readme`, `keywords`, `categories`.

### C — Arduino Library Registry channel

- [ ] **111.C.1** — `tools/release/arduino-pack.sh` builds `libnros.a` per supported arch (Phase 23 chooses initial set: ESP32, ESP32-S3, ESP32-C3), assembles `library.properties`, headers, `examples/Talker/Talker.ino`, `examples/Listener/Listener.ino`.
- [ ] **111.C.2** — GH Actions job invokes pack script on tag, attaches zip to GH Release.
- [ ] **111.C.3** — Submit to Arduino Library Registry per `arduino/library-registry` PR process. Document the resubmit-on-version-bump flow.
- [ ] **111.C.4** — `book/src/getting-started/arduino.md` — install via Library Manager, paste sketch, upload, watch zenohd.

**Files**:
- `tools/release/arduino-pack.sh` (new)
- `tools/release/arduino/library.properties.in` (new)
- `tools/release/arduino/keywords.txt.in` (new)
- `tools/release/arduino/examples/` (new, mirrors `examples/qemu-arm-freertos/c/zenoh/` use cases adapted for Arduino)
- `.github/workflows/release-publish-arduino.yml` (new)
- `book/src/getting-started/arduino.md` (new)

### D — ESP-IDF Component Registry channel

- [ ] **111.D.1** — `tools/release/idf-pack.sh` produces a component directory: `idf_component.yml` + `CMakeLists.txt` + per-target precompiled `libnros.a` under `lib/<target>/` + headers under `include/`.
- [ ] **111.D.2** — `compote component upload --namespace nano-ros --name nros` from a CI runner with `IDF_COMPONENT_API_TOKEN` secret.
- [ ] **111.D.3** — Sample IDF project at `examples/esp-idf/talker/` (new — distinct from existing bare-metal `examples/esp32/`). Uses `idf_component.yml` to depend on the published component.
- [ ] **111.D.4** — `book/src/getting-started/esp-idf.md` — `idf.py create-project`, edit `main/idf_component.yml`, build flash monitor.

**Files**:
- `tools/release/idf-pack.sh` (new)
- `tools/release/idf/idf_component.yml.in` (new)
- `examples/esp-idf/talker/` (new)
- `.github/workflows/release-publish-idf.yml` (new)
- `book/src/getting-started/esp-idf.md` (new)

### E — PlatformIO Library channel

- [ ] **111.E.1** — `tools/release/pio-pack.sh` produces a PIO library: `library.json` + same per-arch `.a` collection as Arduino.
- [ ] **111.E.2** — `pio package publish` from CI. Tag-triggered.
- [ ] **111.E.3** — Sample PIO project at `examples/platformio/talker/` with `platformio.ini` env matrix.
- [ ] **111.E.4** — `book/src/getting-started/platformio.md`.

**Files**:
- `tools/release/pio-pack.sh` (new)
- `tools/release/pio/library.json.in` (new)
- `examples/platformio/talker/` (new)
- `.github/workflows/release-publish-pio.yml` (new)
- `book/src/getting-started/platformio.md` (new)

### F — Zephyr west module channel

- [ ] **111.F.1** — Audit `west.yml` and `zephyr/module.yml`. Verify the repo can be consumed as a foreign module via a one-line `west.yml` snippet without `[patch.crates-io]` (depends on Pillar B publish-to-crates.io).
- [ ] **111.F.2** — Document the `west init -m https://github.com/newslabntu/nano-ros …` flow.
- [ ] **111.F.3** — `examples/zephyr-as-foreign-module/` — a sample external project that adds nano-ros via west.

**Files**:
- `book/src/getting-started/zephyr.md` (rewrite the "Setup" section)
- `examples/zephyr-as-foreign-module/` (new)

### G — GitHub Releases tarball + agent Docker images

- [ ] **111.G.1** — `tools/release/source-tarball.sh` produces `nano-ros-<ver>.tar.gz` with the relocatable `find_package(NanoRos)` install (Phase 75 product).
- [ ] **111.G.2** — Dockerfiles for `nano-ros-zenoh-router` (wraps `zenohd`) and `nano-ros-xrce-agent` (wraps `MicroXRCEAgent`). Multi-arch (amd64, arm64).
- [ ] **111.G.3** — GH Actions `release-publish-docker.yml` builds and pushes to GHCR.
- [ ] **111.G.4** — Mention `docker run …` in every getting-started page replacing the "build zenohd from source" steps.

**Files**:
- `tools/release/source-tarball.sh` (new)
- `docker/agents/zenoh-router.Dockerfile` (new — `docker/` already exists)
- `docker/agents/xrce-agent.Dockerfile` (new)
- `.github/workflows/release-publish-docker.yml` (new)
- `book/src/getting-started/{freertos,zephyr,nuttx,esp32,threadx,bare-metal}.md` (update agent sections)

### H — Documentation consolidation

- [ ] **111.H.1** — `book/src/getting-started/your-first-project.md` — the single 5-command "hello world" page (`nros new` → `nros build` → `nros run`).
- [ ] **111.H.2** — Per-RTOS pages in `book/src/getting-started/` retained but slimmed to RTOS-specific notes (network, SDK download, debugging) — move generic boilerplate up to the new quickstart.
- [ ] **111.H.3** — `book/src/reference/cli.md` — full `nros` reference, generated from `nros --help-all` + manual prose.
- [ ] **111.H.4** — `docs/development/release.md` — release process for maintainers.

**Files**:
- `book/src/getting-started/your-first-project.md` (new)
- `book/src/getting-started/{freertos,zephyr,nuttx,esp32,threadx,bare-metal,arduino,esp-idf,platformio}.md` (update or new)
- `book/src/reference/cli.md` (new)
- `docs/development/release.md` (new)
- `book/src/SUMMARY.md` (update TOC)

---

## Acceptance criteria

A — `nros` CLI:
- `nros new --platform freertos --rmw zenoh --lang c talker` produces a directory that builds with `nros build` and runs with `nros run` on a fresh clone, on both Linux and macOS hosts.
- `nros doctor` exits 0 on a healthy `just setup`'d workspace; exits non-zero with a fixit hint when a SDK path is missing.
- `nros generate c` regenerates the same C bindings as `cargo nano-ros generate-c` byte-for-byte.
- `nros board list` enumerates ≥ 9 boards with chip/flash/ram populated.
- The `cargo nano-ros` legacy entry point still works and goes through the same code paths.

B — Rust crates.io:
- All ~30 workspace crates have crates.io-required metadata fields populated.
- `nros release detect` produces a stable, sorted publish plan on a fresh clone.
- A `0.1.0` initial publish completes and `cargo new && cargo add nros` works against the published version.
- Every example's `[patch.crates-io]` block is removed; examples build against published crates.
- The `release-version-check` PR guard catches a deliberately-broken version-bump scenario in a test fixture.

C–G — Per-channel:
- Each channel has a green CI run on the `v0.1.0` tag.
- Each channel's getting-started page has been smoke-tested by following it verbatim from a clean machine and reaching "received message on /chatter".

H — Docs:
- The new "Your first project" page is ≤ 80 lines and works end-to-end on POSIX, FreeRTOS-QEMU, and Zephyr `native_sim`.
- `book/src/SUMMARY.md` reorganized so a new user reaches the quickstart in ≤ 2 clicks.

---

## Notes

- **Risk: cargo crate-name squatting.** Names like `nros`, `zpico-sys` are short enough to be at risk. Reserve them via a `0.0.0` placeholder publish before the real `0.1.0`.
- **Risk: per-channel maintenance burden.** Six channels × release. Mitigation: each channel must have its publish step be a single `tools/release/<channel>-pack.sh` invocation with no manual steps. CI matrix runs them in parallel.
- **Risk: Phase 23 dependency.** Channel C (Arduino) overlaps with Phase 23. Phase 111 owns the *release pipeline*; Phase 23 owns the *library content*. Sequence: Phase 23 lands the library and `<NanoROS.h>` API → Phase 111 wraps it in the release pipeline.
- **Risk: precompiled `.a` per arch is an ABI commitment.** A `nros-c` ABI break forces all C/C++ channel re-publishes. Use cbindgen-generated headers (already in place per CLAUDE.md) so header drift is detectable; gate ABI-relevant changes behind a `0.x` major-bump policy until 1.0.
- **Out of scope (move to follow-up phases):** `nano-ros.toml` config-file unification, package registry / `nros add`, board descriptor TOML, runtime transport vtable. Tracked in `docs/research/sdk-ux/SYNTHESIS.md`.
- **Out of scope:** Conan / vcpkg / Debian / RPM packaging. Re-evaluate after the six core channels are landed and we have actual user demand signal.
- **Sequencing within phase:** A.1–A.5 (CLI scaffolder, generate, config) → B (crates.io publish) blocks F (Zephyr west clean), C/D/E (Arduino, IDF, PIO). G can land in parallel with B. H lands last as it documents the result.
